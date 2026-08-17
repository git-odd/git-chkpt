use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn run(cwd: &Path, program: &str, args: &[&str]) -> Output {
    Command::new(program)
        .current_dir(cwd)
        .args(args)
        .env("GIT_CHKPT_FOSSIL", "fossil")
        .output()
        .unwrap_or_else(|err| panic!("failed to run {program}: {err}"))
}

fn run_ok(cwd: &Path, program: &str, args: &[&str]) -> String {
    let output = run(cwd, program, args);
    if !output.status.success() {
        panic!(
            "command failed: {program} {:?}\nstdout:\n{}\nstderr:\n{}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn run_fail(cwd: &Path, program: &str, args: &[&str]) -> String {
    let output = run(cwd, program, args);
    if output.status.success() {
        panic!(
            "command unexpectedly succeeded: {program} {:?}\nstdout:\n{}\nstderr:\n{}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, text).unwrap();
}

fn save_id(output: &str) -> String {
    output
        .lines()
        .find_map(|line| line.strip_prefix("Saved checkpoint "))
        .unwrap_or_else(|| panic!("save output did not contain checkpoint id: {output}"))
        .trim()
        .to_owned()
}

fn assert_no_created_timezone(list: &str) {
    for line in list.lines().skip(1).filter(|line| !line.trim().is_empty()) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        assert!(
            fields.len() >= 4,
            "list row should have at least 4 fields: {line}"
        );
        assert!(
            matches!(fields[3], "manual" | "pre-restore:restore"),
            "CREATED should not include a timezone token: {line}"
        );
    }
}

fn init_repo(repo: &Path) {
    fs::create_dir_all(repo).unwrap();
    run_ok(repo, "git", &["init"]);
}

fn commit_all(repo: &Path, message: &str) {
    run_ok(repo, "git", &["add", "."]);
    run_ok(
        repo,
        "git",
        &[
            "-c",
            "user.name=git-chkpt-test",
            "-c",
            "user.email=git-chkpt-test@example.invalid",
            "commit",
            "-m",
            message,
        ],
    );
}

#[test]
fn save_diff_restore_preserves_ignored_files() {
    let bin = env!("CARGO_BIN_EXE_git-chkpt");
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();

    init_repo(repo);
    write(&repo.join(".gitignore"), "ignored/\n");
    write(&repo.join("a.txt"), "A\n");
    write(&repo.join("b.txt"), "B\n");
    write(&repo.join("note.txt"), "note\n");
    write(&repo.join("ignored/cache.bin"), "CACHE-1\n");
    run_ok(repo, "git", &["add", ".gitignore", "a.txt", "b.txt"]);

    let save = run_ok(repo, bin, &["save", "first checkpoint"]);
    assert!(save.contains("Saved checkpoint"), "{save}");

    write(&repo.join("a.txt"), "X\n");
    fs::remove_file(repo.join("b.txt")).unwrap();
    write(&repo.join("c.txt"), "C\n");
    write(&repo.join("ignored/cache.bin"), "CACHE-2\n");

    let diff = run_ok(repo, bin, &["diff"]);
    assert!(diff.contains("M  a.txt"), "{diff}");
    assert!(diff.contains("D  b.txt"), "{diff}");
    assert!(diff.contains("A  c.txt"), "{diff}");

    let restore = run_ok(repo, bin, &["restore"]);
    assert!(
        restore.contains("Saved current workspace as checkpoint"),
        "{restore}"
    );
    assert!(restore.contains("Restored checkpoint"), "{restore}");

    assert_eq!(fs::read_to_string(repo.join("a.txt")).unwrap(), "A\n");
    assert_eq!(fs::read_to_string(repo.join("b.txt")).unwrap(), "B\n");
    assert_eq!(fs::read_to_string(repo.join("note.txt")).unwrap(), "note\n");
    assert!(!repo.join("c.txt").exists());
    assert_eq!(
        fs::read_to_string(repo.join("ignored/cache.bin")).unwrap(),
        "CACHE-2\n"
    );

    let list = run_ok(repo, bin, &["ls"]);
    assert!(list.contains("manual"), "{list}");
    assert!(list.contains("pre-restore:restore"), "{list}");
    assert_no_created_timezone(&list);
}

#[test]
fn restore_is_reversible_with_pre_restore_checkpoint() {
    let bin = env!("CARGO_BIN_EXE_git-chkpt");
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();

    init_repo(repo);
    write(&repo.join("a.txt"), "A\n");
    run_ok(repo, "git", &["add", "a.txt"]);
    run_ok(repo, bin, &["save", "state A"]);

    write(&repo.join("a.txt"), "B\n");
    write(&repo.join("b.txt"), "B-only\n");
    run_ok(repo, bin, &["restore"]);
    assert_eq!(fs::read_to_string(repo.join("a.txt")).unwrap(), "A\n");
    assert!(!repo.join("b.txt").exists());

    let list = run_ok(repo, bin, &["ls"]);
    assert!(list.contains("pre-restore:restore"), "{list}");

    run_ok(repo, bin, &["restore"]);
    assert_eq!(fs::read_to_string(repo.join("a.txt")).unwrap(), "B\n");
    assert_eq!(fs::read_to_string(repo.join("b.txt")).unwrap(), "B-only\n");
}

#[test]
fn delete_hides_checkpoint_from_public_commands() {
    let bin = env!("CARGO_BIN_EXE_git-chkpt");
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();

    init_repo(repo);
    write(&repo.join("a.txt"), "A\n");
    let id = save_id(&run_ok(repo, bin, &["save", "temporary checkpoint"]));

    let list = run_ok(repo, bin, &["ls"]);
    assert!(list.contains("temporary checkpoint"), "{list}");

    let delete = run_ok(repo, bin, &["rm", &id]);
    assert!(delete.contains("Deleted checkpoint"), "{delete}");

    let list_after = run_ok(repo, bin, &["ls"]);
    assert!(!list_after.contains("temporary checkpoint"), "{list_after}");

    let show = run_fail(repo, bin, &["show", &id]);
    assert!(
        show.contains("no-checkpoint") || show.contains("checkpoint-not-found"),
        "{show}"
    );
}

#[test]
fn save_does_not_modify_git_status_or_index() {
    let bin = env!("CARGO_BIN_EXE_git-chkpt");
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();

    init_repo(repo);
    write(&repo.join("tracked.txt"), "tracked\n");
    run_ok(repo, "git", &["add", "tracked.txt"]);
    write(&repo.join("untracked.txt"), "untracked\n");

    let status_before = run_ok(repo, "git", &["status", "--porcelain=v2"]);
    let index_before = fs::read(repo.join(".git/index")).unwrap();
    run_ok(repo, bin, &["save", "no side effects"]);
    let status_after = run_ok(repo, "git", &["status", "--porcelain=v2"]);
    let index_after = fs::read(repo.join(".git/index")).unwrap();

    assert_eq!(status_before, status_after);
    assert_eq!(index_before, index_after);
    assert_eq!(
        fs::read_to_string(repo.join("untracked.txt")).unwrap(),
        "untracked\n"
    );
}

#[test]
fn restore_does_not_modify_git_index_bytes() {
    let bin = env!("CARGO_BIN_EXE_git-chkpt");
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();

    init_repo(repo);
    write(&repo.join("tracked.txt"), "A\n");
    run_ok(repo, "git", &["add", "tracked.txt"]);
    run_ok(repo, bin, &["save", "state A"]);

    write(&repo.join("tracked.txt"), "B\n");
    run_ok(repo, "git", &["add", "tracked.txt"]);
    let index_before = fs::read(repo.join(".git/index")).unwrap();

    run_ok(repo, bin, &["restore"]);
    let index_after = fs::read(repo.join(".git/index")).unwrap();

    assert_eq!(index_before, index_after);
    assert_eq!(fs::read_to_string(repo.join("tracked.txt")).unwrap(), "A\n");
}

#[test]
fn restore_preserves_ignored_children_inside_deleted_dirs() {
    let bin = env!("CARGO_BIN_EXE_git-chkpt");
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();

    init_repo(repo);
    write(&repo.join(".gitignore"), "build/cache.bin\n");
    write(&repo.join("a.txt"), "A\n");
    run_ok(repo, "git", &["add", ".gitignore", "a.txt"]);
    run_ok(repo, bin, &["save", "clean"]);

    write(&repo.join("build/old.rs"), "managed temporary\n");
    write(&repo.join("build/cache.bin"), "ignored cache\n");
    run_ok(repo, bin, &["restore"]);

    assert!(!repo.join("build/old.rs").exists());
    assert_eq!(
        fs::read_to_string(repo.join("build/cache.bin")).unwrap(),
        "ignored cache\n"
    );
}

#[test]
fn linked_worktrees_have_isolated_checkpoint_storage() {
    let bin = env!("CARGO_BIN_EXE_git-chkpt");
    let temp = tempfile::tempdir().unwrap();
    let main = temp.path().join("main");
    let linked = temp.path().join("linked");

    init_repo(&main);
    write(&main.join("base.txt"), "base\n");
    commit_all(&main, "init");

    run_ok(
        &main,
        "git",
        &["worktree", "add", "-b", "feature", linked.to_str().unwrap()],
    );

    write(&main.join("main.txt"), "main checkpoint\n");
    write(&linked.join("linked.txt"), "linked checkpoint\n");
    run_ok(&main, bin, &["save", "main checkpoint"]);
    run_ok(&linked, bin, &["save", "linked checkpoint"]);

    let main_list = run_ok(&main, bin, &["list"]);
    let linked_list = run_ok(&linked, bin, &["list"]);
    assert!(main_list.contains("main checkpoint"), "{main_list}");
    assert!(!main_list.contains("linked checkpoint"), "{main_list}");
    assert!(linked_list.contains("linked checkpoint"), "{linked_list}");
    assert!(!linked_list.contains("main checkpoint"), "{linked_list}");

    assert!(main.join(".git/git-chkpt/repository.fossil").is_file());
    assert!(
        main.join(".git/worktrees/linked/git-chkpt/repository.fossil")
            .is_file()
    );
}

#[test]
fn restore_handles_file_directory_type_changes() {
    let bin = env!("CARGO_BIN_EXE_git-chkpt");
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();

    init_repo(repo);
    write(&repo.join("node"), "file A\n");
    run_ok(repo, bin, &["save", "file node"]);

    fs::remove_file(repo.join("node")).unwrap();
    write(&repo.join("node/child.txt"), "directory B\n");
    run_ok(repo, bin, &["restore"]);

    assert_eq!(fs::read_to_string(repo.join("node")).unwrap(), "file A\n");
    assert!(!repo.join("node/child.txt").exists());

    write(&repo.join("dir/child.txt"), "directory A\n");
    run_ok(repo, bin, &["save", "directory node"]);
    fs::remove_dir_all(repo.join("dir")).unwrap();
    write(&repo.join("dir"), "file B\n");
    run_ok(repo, bin, &["restore"]);

    assert_eq!(
        fs::read_to_string(repo.join("dir/child.txt")).unwrap(),
        "directory A\n"
    );
}

#[test]
fn parent_checkpoint_does_not_enter_real_submodule() {
    let bin = env!("CARGO_BIN_EXE_git-chkpt");
    let temp = tempfile::tempdir().unwrap();
    let sub_source = temp.path().join("sub-source");
    let parent = temp.path().join("parent");

    init_repo(&sub_source);
    write(&sub_source.join("lib.txt"), "submodule committed\n");
    commit_all(&sub_source, "sub init");

    init_repo(&parent);
    write(&parent.join("parent.txt"), "parent A\n");
    commit_all(&parent, "parent init");
    run_ok(
        &parent,
        "git",
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            sub_source.to_str().unwrap(),
            "vendor/lib",
        ],
    );

    write(&parent.join("vendor/lib/lib.txt"), "submodule dirty A\n");
    run_ok(&parent, bin, &["save", "parent with submodule"]);

    write(&parent.join("parent.txt"), "parent B\n");
    write(&parent.join("vendor/lib/lib.txt"), "submodule dirty B\n");
    run_ok(&parent, bin, &["restore"]);

    assert_eq!(
        fs::read_to_string(parent.join("parent.txt")).unwrap(),
        "parent A\n"
    );
    assert_eq!(
        fs::read_to_string(parent.join("vendor/lib/lib.txt")).unwrap(),
        "submodule dirty B\n"
    );
}

#[test]
fn read_commands_do_not_initialize_checkpoint_storage() {
    let bin = env!("CARGO_BIN_EXE_git-chkpt");
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();

    init_repo(repo);

    let list = run_ok(repo, bin, &["list"]);
    assert!(list.trim().is_empty(), "{list}");
    assert!(!repo.join(".git/git-chkpt/repository.fossil").exists());

    let show = run_fail(repo, bin, &["show"]);
    assert!(show.contains("no-checkpoint"), "{show}");
    let diff = run_fail(repo, bin, &["diff"]);
    assert!(diff.contains("no-checkpoint"), "{diff}");
    assert!(!repo.join(".git/git-chkpt/repository.fossil").exists());
}

#[test]
fn default_command_saves_checkpoint() {
    let bin = env!("CARGO_BIN_EXE_git-chkpt");
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();

    init_repo(repo);
    write(&repo.join("a.txt"), "A\n");

    let save = run_ok(repo, bin, &[]);
    assert!(save.contains("Saved checkpoint"), "{save}");
    let list = run_ok(repo, bin, &["list"]);
    assert!(list.contains("manual"), "{list}");
}

#[test]
fn checkpoints_follow_git_directory_lifecycle() {
    let bin = env!("CARGO_BIN_EXE_git-chkpt");
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();

    init_repo(repo);
    write(&repo.join("a.txt"), "A\n");
    run_ok(repo, bin, &["save", "before reinit"]);
    assert!(repo.join(".git/git-chkpt/repository.fossil").is_file());

    fs::remove_dir_all(repo.join(".git")).unwrap();
    run_ok(repo, "git", &["init"]);

    let list = run_ok(repo, bin, &["list"]);
    assert!(list.trim().is_empty(), "{list}");
    assert!(!repo.join(".git/git-chkpt/repository.fossil").exists());
}

#[test]
fn nested_git_repository_is_not_snapshotted_or_restored() {
    let bin = env!("CARGO_BIN_EXE_git-chkpt");
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();
    let nested = repo.join("vendor/lib");

    init_repo(repo);
    write(&repo.join("src.txt"), "parent A\n");
    fs::create_dir_all(&nested).unwrap();
    run_ok(&nested, "git", &["init"]);
    write(&nested.join("inner.txt"), "nested A\n");

    run_ok(repo, bin, &["save", "parent only"]);

    write(&repo.join("src.txt"), "parent B\n");
    write(&nested.join("inner.txt"), "nested B\n");
    run_ok(repo, bin, &["restore"]);

    assert_eq!(
        fs::read_to_string(repo.join("src.txt")).unwrap(),
        "parent A\n"
    );
    assert_eq!(
        fs::read_to_string(nested.join("inner.txt")).unwrap(),
        "nested B\n"
    );
}

#[test]
fn fossil_resolves_from_path_without_auto_download() {
    let bin = env!("CARGO_BIN_EXE_git-chkpt");
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let cache_dir = temp.path().join("cache");

    init_repo(&repo);
    write(&repo.join("a.txt"), "A\n");

    let mut command = Command::new(bin);
    command
        .current_dir(&repo)
        .args(["save", "path resolution test"])
        .env_remove("GIT_CHKPT_FOSSIL")
        .env("GIT_CHKPT_FOSSIL_RUNTIME_CACHE", &cache_dir);
    let output = command.output().expect("execute git-chkpt");
    if !output.status.success() {
        panic!(
            "failed: stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Saved checkpoint"), "{stdout}");
    assert!(!cache_dir.exists() || fs::read_dir(&cache_dir).unwrap().next().is_none());
}

#[test]
fn git_checkpoint_shim_binary_works() {
    let bin = env!("CARGO_BIN_EXE_git-checkpoint");
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();

    init_repo(repo);
    write(&repo.join("a.txt"), "A\n");

    let save = run_ok(repo, bin, &["save", "via shim"]);
    assert!(save.contains("Saved checkpoint"), "{save}");
    let list = run_ok(repo, bin, &["list"]);
    assert!(list.contains("via shim"), "{list}");
}
