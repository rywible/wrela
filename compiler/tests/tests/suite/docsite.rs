//! The docs site (docsite): its readers of JSON and Markdown, its layout, search and routes, and
//! a frame test that drives its search and find with a script of keys; and the site in Chrome,
//! read through its semantic tree.
#[test]
fn the_docs_site_passes_its_tests() {
    let count = super::tests_pass(&wrela_tests::repo_root().join("docsite"));
    assert!(count >= 14, "{count} tests");
}

/// The docs site's page, with the small corpus (docsite/scripts/corpus) beside it.
fn docsite_page(name: &str) -> (std::path::PathBuf, String) {
    let (dir, rel) = wrela_tests::page("docsite", name);
    let corpus = wrela_tests::repo_root().join("docsite/scripts/corpus");
    for file in wrela_tests::files_under(&corpus, &["json", "md"]) {
        let to = dir.join(file.strip_prefix(&corpus).expect("in the corpus"));
        std::fs::create_dir_all(to.parent().expect("a directory")).expect("mkdir");
        std::fs::copy(&file, &to).expect("copy");
    }
    (dir, rel)
}

const FRAGMENT: &str = "#test&frames=900&width=1400&height=900";

/// In Chrome, over a small corpus (docsite/scripts/corpus), the docs are real links: the list of
/// pages opens a page, which names the tab, has its sections as an outline's links and its next
/// page as a link, which opens that page; a copy button and the page's copy reach the clipboard
/// (the browser script `scripts::READER`). No tree is refused.
#[test]
#[ignore = "needs the runtime"]
fn the_docs_in_chrome_are_links_an_outline_and_titles() {
    let (dir, rel) = docsite_page("docsite-chrome");
    let script = wrela_tests::browser_script("docsite", "READER");
    assert!(
        wrela_tests::run_page_with_script(&rel, FRAGMENT, &script, 300),
        "the browser run failed; see {rel}/results/console.log"
    );
    let log = std::fs::read_to_string(dir.join("results/log.txt")).unwrap_or_default();
    assert!(!log.contains("isn't sent"), "{log}");
}

/// The reader script's expectations are checked: with the outline it waits for changed, the run
/// fails, saying what the page had.
#[test]
#[ignore = "needs the runtime"]
fn the_docs_scripts_outline_is_an_expectation_that_fails_when_wrong() {
    use wrela_host::chrome::script::{Step, Until};
    let (_, rel) = docsite_page("docsite-chrome-wrong");
    let mut script = wrela_tests::browser_script("docsite", "READER");
    let wrong = script.iter_mut().find_map(|s| match s {
        Step::Wait(Until::Texts(_, are), timeout) => {
            *timeout = std::time::Duration::from_secs(3);
            Some(are)
        }
        _ => None,
    });
    let are = wrong.expect("the script waits for the outline");
    are[1] = "6.2 Parameter moods".into();
    let failed =
        wrela_tests::run_page(&rel, FRAGMENT, 300, Some(&script)).expect_err("a wrong outline");
    assert!(
        failed.contains("Parameter moods") && failed.contains("6.2 Parameter modes"),
        "{failed}"
    );
}
