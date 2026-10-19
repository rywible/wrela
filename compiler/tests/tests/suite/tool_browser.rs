//! `std::tool::browser` (language.md §15.5) on the tool host, in real Chrome stable on the
//! hardware GPU: a tool serves pages from its scratch root (`std::tool::serve`), opens them, acts
//! on them with typed scripts (trusted clicks and keys, an input method, permissions, focus) and
//! reads what they show (texts, counts, attributes, the focus, the location, the console, the
//! clipboard, the test-mode result, the screen). Its pages reach only its own servers: a fetch to
//! another local port, from the page or its worker, and one to a host it isn't granted, are
//! refused. The clipboard is read only once it's granted. A page that never reports fails its
//! wait at the wait's timeout, and the request at its deadline, in bounded time. However the tool
//! ends, a trap with a browser open among them, no Chrome process and no profile is left.

use crate::tool::{printer, run, scratch, tool_build};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use wrela_host::tool::{Caps, End, Ended, Grants, Signal, Testing};

/// A tool that serves its scratch root, opens the page its second argument names there, and does
/// what its first says:
/// - `act`: types into the form and clicks its button, then prints what it observes;
/// - `refused`: waits until the page has tried what it may not reach, and prints its console;
/// - `clipboard`: reads the clipboard before and after a script grants it and clicks the copy
///   button;
/// - `done`: waits for the page's result, and prints it;
/// - `never`: waits for a result that never comes, by its wait's timeout, then by the request's
///   deadline;
/// - `trap`: panics with the browser open.
///
/// Each failure prints why, and the tool exits 1.
const BROWSER_TOOL: &str = r##"
use std::input::{Key, Mods}
use std::io::print
use std::job::{Job, Step}
use std::tool::{RUNNING, answer, args}
use std::tool::blob
use std::tool::blob::Data
use std::tool::browser
use std::tool::browser::{Element, Match, Permission, Script, Until}
use std::tool::serve

@job
fn main(given: take Vec<String>) -> u32 {
    let what = given[0].clone()
    let page = given[1].clone()
    var s = serve::start("scratch")
    var sg = s.poll()
    while sg.is_none() {
        yield
        sg = s.poll()
    }
    let Some(sp) = take sg else { return 90 }
    let started = match serve::read_started(take sp) {
        Ok(x) => take x,
        Err(e) => {
            print(f"serve: {e.message()}")
            return 1
        },
    }
    let url = f"{started.origin.as_str()}/{page.as_str()}"
    var o = browser::open(url.as_str(), width: 320, height: 240)
    var og = o.poll()
    while og.is_none() {
        yield
        og = o.poll()
    }
    let Some(op) = take og else { return 90 }
    let b = match browser::read_opened(take op) {
        Ok(b) => b,
        Err(e) => {
            print(f"open: {e.message()}")
            return 1
        },
    }
    print("opened")
    if what.as_str() == "trap" {
        panic("a trap with a browser open")
    }
    var script = Script::new()
    if what.as_str() == "act" {
        script = Script::new()
            .wait(Until::title(Match::is("Form")))
            .click(Element::css("#name"))
            .wait(Until::active("input"))
            .press(Key::A)
            .press(Key::B, Mods { shift: true })
            .insert_text("é")
            .wait(Until::title(Match::is("typed aBé")))
            .click(Element::css("button").with_text("Go"))
            .wait(Until::exists(Element::css("#out").with_text("clicked")))
    }
    if what.as_str() == "refused" {
        script = Script::new().wait(Until::title(Match::starts("tried")))
    }
    if what.as_str() == "clipboard" {
        var c = browser::clipboard(b)
        var cg = c.poll()
        while cg.is_none() {
            yield
            cg = c.poll()
        }
        let Some(cp) = take cg else { return 90 }
        match browser::read_text(take cp) {
            Ok(t) => print(f"clipboard before: {t.as_str()}"),
            Err(_) => print("clipboard before: refused"),
        }
        script = Script::new()
            .grant(Permission::Clipboard)
            .emulate_focus()
            .click(Element::css("button").with_text("Copy"))
            .wait(Until::clipboard(Match::is("copied!")))
    }
    if what.as_str() == "done" {
        script = Script::new().wait(Until::done())
    }
    if what.as_str() == "never" {
        script = Script::new().wait(Until::done(), timeout: 0.3)
    }
    var r = browser::run(b, script)
    var rg = r.poll()
    while rg.is_none() {
        yield
        rg = r.poll()
    }
    let Some(rp) = take rg else { return 90 }
    if let Err(e) = answer(take rp) {
        print(f"run: {e.message()}")
        if what.as_str() != "never" {
            return 1
        }
        // The request's own deadline, sooner than its wait's.
        var d = browser::run(b, Script::new().wait(Until::done(), timeout: 30.0), timeout: 0.5)
        var dg = d.poll()
        while dg.is_none() {
            yield
            dg = d.poll()
        }
        let Some(dp) = take dg else { return 90 }
        if let Err(e) = answer(take dp) {
            print(f"deadline: {e.message()}")
        }
        return 0
    }
    if what.as_str() == "act" {
        var t = browser::title(b)
        var tg = t.poll()
        while tg.is_none() {
            yield
            tg = t.poll()
        }
        let Some(tp) = take tg else { return 90 }
        let Ok(title) = browser::read_text(take tp) else { return 2 }
        print(f"title {title.as_str()}")
        var x = browser::texts(b, "li")
        var xg = x.poll()
        while xg.is_none() {
            yield
            xg = x.poll()
        }
        let Some(xp) = take xg else { return 90 }
        let Ok(items) = browser::read_texts(take xp) else { return 2 }
        print(f"items {items.len()} {items[0].as_str()} {items[1].as_str()}")
        var n = browser::count(b, Element::css("li").with_text("two"))
        var ng = n.poll()
        while ng.is_none() {
            yield
            ng = n.poll()
        }
        let Some(np) = take ng else { return 90 }
        let Ok(count) = browser::read_count(take np) else { return 2 }
        print(f"count {count}")
        var a = browser::attribute(b, Element::css("#next"), "href")
        var ag = a.poll()
        while ag.is_none() {
            yield
            ag = a.poll()
        }
        let Some(ap) = take ag else { return 90 }
        let Ok(Some(href)) = browser::read_attribute(take ap) else { return 2 }
        print(f"href {href.as_str()}")
        var f = browser::active(b)
        var fg = f.poll()
        while fg.is_none() {
            yield
            fg = f.poll()
        }
        let Some(fp) = take fg else { return 90 }
        let Ok(Some(focused)) = browser::read_active(take fp) else { return 2 }
        print(f"active {focused.tag.as_str()} {focused.id.as_str()}")
        var l = browser::location(b)
        var lg = l.poll()
        while lg.is_none() {
            yield
            lg = l.poll()
        }
        let Some(lp) = take lg else { return 90 }
        let Ok(location) = browser::read_text(take lp) else { return 2 }
        print(f"page {Match::ends("/form.html").matches(location.as_str())}")
        var k = browser::capture(b)
        var kg = k.poll()
        while kg.is_none() {
            yield
            kg = k.poll()
        }
        let Some(kp) = take kg else { return 90 }
        let capture = match browser::read_capture(take kp) {
            Ok(c) => take c,
            Err(e) => {
                print(f"capture: {e.message()}")
                return 1
            },
        }
        let Data::Blob(png) = capture.png else { return 3 }
        var h = blob::read(png, 0, 8)
        var hg = h.poll()
        while hg.is_none() {
            yield
            hg = h.poll()
        }
        let Some(hp) = take hg else { return 90 }
        let Ok(head) = answer(take hp) else { return 2 }
        print(f"capture {capture.width}x{capture.height} png {head.len() == 8 && head[1] == 80 && head[2] == 78 && head[3] == 71}")
    }
    if what.as_str() == "refused" || what.as_str() == "act" {
        var c = browser::console(b)
        var cg = c.poll()
        while cg.is_none() {
            yield
            cg = c.poll()
        }
        let Some(cp) = take cg else { return 90 }
        let Ok(lines) = browser::read_texts(take cp) else { return 2 }
        for line in lines {
            print(f"console {line.as_str()}")
        }
        var t = browser::title(b)
        var tg = t.poll()
        while tg.is_none() {
            yield
            tg = t.poll()
        }
        let Some(tp) = take tg else { return 90 }
        let Ok(title) = browser::read_text(take tp) else { return 2 }
        print(f"title {title.as_str()}")
    }
    if what.as_str() == "clipboard" {
        var c = browser::clipboard(b)
        var cg = c.poll()
        while cg.is_none() {
            yield
            cg = c.poll()
        }
        let Some(cp) = take cg else { return 90 }
        let Ok(t) = browser::read_text(take cp) else { return 2 }
        print(f"clipboard after: {t.as_str()}")
    }
    if what.as_str() == "done" {
        var d = browser::results(b)
        var dg = d.poll()
        while dg.is_none() {
            yield
            dg = d.poll()
        }
        let Some(dp) = take dg else { return 90 }
        let Ok(Some(done)) = browser::read_attribute(take dp) else { return 2 }
        print(f"done {done.as_str()}")
    }
    var z = browser::close(b)
    var zg = z.poll()
    while zg.is_none() {
        yield
        zg = z.poll()
    }
    let Some(zp) = take zg else { return 90 }
    if let Err(e) = answer(take zp) {
        print(f"close: {e.message()}")
        return 1
    }
    print("closed")
    0
}

pub fn tool_init() -> Job<main> {
    main.start(args())
}

pub fn tool(job: mut Job<main>) -> u32 {
    match job.resume() {
        Step::Yielded => RUNNING,
        Step::Done(status) => status,
    }
}
"##;

const FORM: &str = r##"<!doctype html>
<title>Form</title>
<input id="name"><button onclick="document.getElementById('out').textContent = 'clicked'">Go</button>
<p id="out"></p>
<ul><li>one</li><li>two</li></ul>
<a id="next" href="#next">Next</a>
<script>
document.getElementById("name").addEventListener("input", (e) => { document.title = "typed " + e.target.value; });
console.log("ready");
</script>
"##;

const CLIPBOARD: &str = r#"<!doctype html>
<title>Clipboard</title>
<button onclick="navigator.clipboard.writeText('copied!')">Copy</button>
"#;

const DONE: &str = r#"<!doctype html>
<title>Done</title>
<script>fetch("results/DONE", { method: "PUT", body: "ok" });</script>
"#;

const NEVER: &str = "<!doctype html><title>Never</title><p>This page never reports.</p>\n";

/// A page that tries three things it may not reach, from the page and from its worker, and says
/// in its title what each did: `OTHER` is another server's port on this machine.
const REFUSING: &str = r#"<!doctype html>
<title>Refusing</title>
<script>
const other = "http://127.0.0.1:OTHER/x";
const tried = (u) => fetch(u).then(() => "reached", () => "refused");
const worker = new Worker(URL.createObjectURL(new Blob(
  [`fetch("${other}/w").then(() => postMessage("reached"), () => postMessage("refused"));`],
  { type: "text/javascript" })));
const fromWorker = new Promise((done) => { worker.onmessage = (e) => done(e.data); });
Promise.all([tried(other), tried("https://example.com/"), fromWorker])
  .then((r) => { document.title = "tried " + r.join(" "); });
</script>
"#;

/// The grants a browser tool gets: its scratch root, and Chrome.
fn grants(root: &Path) -> Grants {
    Grants { browser: true, ..Grants::default() }.root("scratch", root, true)
}

/// Runs the browser tool doing `what` on `page` (written to its scratch root as `name`), with
/// `grants` and `testing`: what it printed, and how it ended.
fn browse(
    test: &str,
    what: &str,
    name: &str,
    page: &str,
    grants: impl FnOnce(&Path) -> Grants,
    testing: Testing,
) -> (Vec<String>, Ended) {
    let root = scratch(&format!("browser-{test}"));
    std::fs::write(root.join(name), page).expect("the page");
    // Built once, for the tests at once.
    static BUILD: std::sync::OnceLock<wrela_host::tool::ToolBuild> = std::sync::OnceLock::new();
    let build = BUILD.get_or_init(|| tool_build("browser-tool", BROWSER_TOOL));
    let (lines, mut options) = printer(|_| {});
    options.caps = Caps::open(&grants(&root), |_| None).expect("granted");
    options.args = vec![what.to_string(), name.to_string()];
    options.timeout = Some(Duration::from_secs(60));
    options.testing = testing;
    let ended = run(build, options);
    let printed = lines.lock().expect("the lines").clone();
    (printed, ended)
}

/// The processes whose command line names `path`.
fn processes_naming(path: &Path) -> Vec<String> {
    let out = std::process::Command::new("ps").args(["-Ao", "pid,command"]).output().expect("ps");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let path = path.display().to_string();
    text.lines().filter(|l| l.contains(&path)).map(str::to_string).collect()
}

#[test]
#[ignore = "needs the runtime"]
fn a_tool_acts_on_a_page_and_reads_what_it_shows() {
    let (printed, ended) = browse("act", "act", "form.html", FORM, grants, Testing::default());
    assert_eq!(ended.end, End::Exit(0), "{printed:?}");
    let want = [
        "opened",
        "title typed aBé",
        "items 2 one two",
        "count 1",
        "href #next",
        "active button ",
        "page true",
        "capture 320x240 png true",
    ];
    assert_eq!(&printed[..want.len()], want, "{printed:?}");
    assert!(printed.contains(&"console ready".to_string()), "{printed:?}");
    assert_eq!(printed.last().map(String::as_str), Some("closed"), "{printed:?}");
    assert_eq!(ended.blobs_released.0, 1, "the capture's blob, which the tool kept");
}

#[test]
#[ignore = "needs the runtime"]
fn a_page_reaches_only_its_tools_servers() {
    // Another server on this machine, which the page must not reach.
    let other = std::net::TcpListener::bind("127.0.0.1:0").expect("listen");
    other.set_nonblocking(true).expect("nonblocking");
    let port = other.local_addr().expect("addr").port();
    let page = REFUSING.replace("OTHER", &port.to_string());
    let (printed, ended) =
        browse("refused", "refused", "refusing.html", &page, grants, Testing::default());
    assert_eq!(ended.end, End::Exit(0), "{printed:?}");
    assert!(printed.contains(&"title tried refused refused refused".to_string()), "{printed:?}");
    let refusals = printed.iter().filter(|l| l.contains("the host refused GET")).count();
    assert!(refusals >= 3, "the page's two and its worker's: {printed:?}");
    assert!(printed.iter().any(|l| l.contains(&format!("127.0.0.1:{port}/x/w"))), "{printed:?}");
    assert!(other.accept().is_err(), "nothing reached the other server");
}

#[test]
#[ignore = "needs the runtime"]
fn the_clipboard_is_read_only_once_it_is_granted() {
    let (printed, ended) =
        browse("clipboard", "clipboard", "clipboard.html", CLIPBOARD, grants, Testing::default());
    assert_eq!(ended.end, End::Exit(0), "{printed:?}");
    let want = ["opened", "clipboard before: refused", "clipboard after: copied!", "closed"];
    assert_eq!(printed, want);
}

#[test]
#[ignore = "needs the runtime"]
fn a_pages_result_is_seen_and_written_to_its_root() {
    let (printed, ended) = browse("done", "done", "done.html", DONE, grants, Testing::default());
    assert_eq!(ended.end, End::Exit(0), "{printed:?}");
    assert_eq!(printed, ["opened", "done ok", "closed"]);
}

#[test]
#[ignore = "needs the runtime"]
fn a_page_that_never_reports_fails_at_its_wait_and_its_deadline() {
    let began = Instant::now();
    let (printed, ended) =
        browse("never", "never", "never.html", NEVER, grants, Testing::default());
    assert_eq!(ended.end, End::Exit(0), "{printed:?}");
    let run = printed.iter().find(|l| l.starts_with("run: ")).expect("the wait failed");
    assert!(run.contains("never had the page's results/DONE within 0.3 s"), "{run}");
    let deadline = printed.iter().find(|l| l.starts_with("deadline: ")).expect("the deadline");
    assert!(deadline.contains("past its deadline of 0.5 s"), "{deadline}");
    assert!(deadline.contains("by the script's deadline"), "{deadline}");
    assert!(began.elapsed() < Duration::from_secs(30), "{:?}", began.elapsed());
}

#[test]
#[ignore = "needs the runtime"]
fn a_browser_needs_its_grant() {
    let ungranted = |root: &Path| Grants::default().root("scratch", root, true);
    let (printed, ended) =
        browse("ungranted", "act", "form.html", FORM, ungranted, Testing::default());
    assert_eq!(ended.end, End::Exit(1), "{printed:?}");
    assert_eq!(printed, ["open: the browser isn't granted: `--browser` grants it"]);
}

/// A tool that traps with a browser open ends with no Chrome process left and its profile gone:
/// while it runs, Chrome's processes name the run's own directory, where its profile is.
#[test]
#[ignore = "needs the runtime"]
fn a_trap_with_a_browser_open_leaves_no_chrome_and_no_profile() {
    let work: PathBuf = scratch("browser-trap-work").join("run");
    // Seen at the moment the browser has opened, before its answer reaches the tool: the host
    // waits for this.
    let (seen, saw) = mpsc::channel();
    let (watched, seen) = (work.clone(), std::sync::Mutex::new(seen));
    let signal = Signal(Arc::new(move |moment| {
        if moment == "browser" {
            let _ = seen.lock().expect("the channel").send(processes_naming(&watched));
        }
    }));
    let testing =
        Testing { work_dir: Some(work.clone()), signal: Some(signal), ..Testing::default() };
    let (printed, ended) = browse("trap", "trap", "never.html", NEVER, grants, testing);
    let End::Trap(why) = &ended.end else { panic!("a trap: {ended:?} {printed:?}") };
    assert!(why.contains("a trap with a browser open"), "{why}");
    let running = saw.recv().expect("what ran");
    assert!(!running.is_empty(), "Chrome ran with its profile in the run's directory");
    assert!(processes_naming(&work).is_empty(), "no Chrome is left: {:?}", processes_naming(&work));
    assert!(!work.exists(), "the run's directory, the profile in it, is gone");
}
