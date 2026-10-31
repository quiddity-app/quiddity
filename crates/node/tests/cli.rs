//! The `quiddity-node` binary end to end: import, serve, sync, export.

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

fn node(graph: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_quiddity-node"));
    command.arg("--graph").arg(graph);
    command
}

/// Runs to completion, returning stdout; panics with stderr on failure.
fn run(command: &mut Command) -> String {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// Every file under `dir`, by relative path.
fn files(dir: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for sub in ["entities", "journals"] {
        for entry in fs::read_dir(dir.join(sub)).unwrap() {
            let path = entry.unwrap().path();
            let name = format!("{sub}/{}", path.file_name().unwrap().to_string_lossy());
            out.insert(name, fs::read_to_string(path).unwrap());
        }
    }
    out
}

#[test]
fn import_serve_sync_export() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b, c) = (
        tmp.path().join("a"),
        tmp.path().join("b"),
        tmp.path().join("c"),
    );

    let logseq = tmp.path().join("logseq-graph");
    fs::create_dir_all(logseq.join("pages")).unwrap();
    fs::create_dir_all(logseq.join("journals")).unwrap();
    fs::write(
        logseq.join("pages/Jane.md"),
        "alias:: JD\n\n- A person\n\t- with a child\n",
    )
    .unwrap();
    fs::write(logseq.join("journals/2026_10_25.md"), "- Met [[JD]]\n").unwrap();

    let a_id = run(node(&a).arg("init"));
    assert_eq!(
        run(node(&a).arg("import").arg(&logseq)),
        "imported 2 entities"
    );
    let b_id = run(node(&b).args(["init", "--blank"]));
    run(node(&c).args(["init", "--blank"]));
    let from_b = tmp.path().join("from-b");
    fs::create_dir_all(from_b.join("pages")).unwrap();
    fs::write(from_b.join("pages/Bob.md"), "- Written on b\n").unwrap();
    let early = node(&b).arg("import").arg(&from_b).output().unwrap();
    assert!(!early.status.success(), "a blank replica must join first");

    // b is let in by being listed in the graph, not by a flag.
    assert_eq!(
        run(node(&a).args(["allow", &b_id])),
        format!("{b_id} may connect")
    );
    let mut server = node(&a)
        .args(["serve", "--local", "127.0.0.1:0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(server.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let addr = line.trim().rsplit(' ').next().unwrap().to_owned();
    assert!(line.starts_with(&format!("serving {a_id} at ")), "{line}");

    let sync = |graph: &Path| {
        node(graph)
            .args(["sync", &a_id, "--addr", &addr, "--local", "127.0.0.1:0"])
            .output()
            .unwrap()
    };
    let synced = |graph: &Path| {
        let output = sync(graph);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{stderr}");
    };
    // b joins, then adds something of its own, which the server must save
    // as it arrives: it's killed, not stopped, so it can't save later.
    synced(&b);
    run(node(&b).arg("import").arg(&from_b));
    synced(&b);
    assert!(!sync(&c).status.success(), "c isn't listed");
    server.kill().unwrap();
    server.wait().unwrap();

    for (graph, out) in [(&a, "a-out"), (&b, "b-out"), (&c, "c-out")] {
        run(node(graph).arg("export").arg(tmp.path().join(out)));
    }
    // Both know both devices: each records itself when it saves, and the
    // records reach the other by syncing. c was refused, and hasn't joined.
    for (graph, me, other) in [(&a, &a_id, &b_id), (&b, &b_id, &a_id)] {
        let devices = run(node(graph).arg("devices"));
        let lines: Vec<&str> = devices.lines().collect();
        assert_eq!(lines.len(), 2, "{devices}");
        let listed = |id: &str, mine: bool| {
            lines
                .iter()
                .any(|l| l.starts_with(id) && l.ends_with("(this device)") == mine)
        };
        assert!(listed(me, true) && listed(other, false), "{devices}");
    }
    assert_eq!(run(node(&c).arg("devices")), "");

    let exported = files(&tmp.path().join("a-out"));
    assert_eq!(exported.len(), 3);
    assert_eq!(files(&tmp.path().join("b-out")), exported);
    assert!(files(&tmp.path().join("c-out")).is_empty());
}

#[test]
fn serve_needs_to_be_told_who_may_connect() {
    let tmp = tempfile::tempdir().unwrap();
    let graph = tmp.path().join("g");
    run(node(&graph).arg("init"));

    let mut server = node(&graph)
        .args(["serve", "--allow-any", "--local", "127.0.0.1:0"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(server.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    server.kill().unwrap();
    server.wait().unwrap();
    assert!(line.starts_with("serving "), "{line:?}");

    // Only itself is listed, so there is nobody to let in. It must say so
    // at once, not serve to no one.
    let mut refused = node(&graph)
        .arg("serve")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let status = loop {
        if let Some(status) = refused.try_wait().unwrap() {
            break Some(status);
        }
        if std::time::Instant::now() > deadline {
            refused.kill().unwrap();
            refused.wait().unwrap();
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert!(
        status.is_some_and(|s| !s.success()),
        "serve should refuse to start"
    );
    let mut stderr = String::new();
    std::io::Read::read_to_string(&mut refused.stderr.take().unwrap(), &mut stderr).unwrap();
    assert!(stderr.contains("allow"), "{stderr}");
}
