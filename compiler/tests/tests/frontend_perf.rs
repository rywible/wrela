use wrela_host::CpuHost;

#[test]
fn frontend_perf_string_append_preserves_bytes_growth_and_ownership() {
    let dir =
        wrela_tests::scratch_root(env!("CARGO_TARGET_TMPDIR")).join("frontend-perf-string-append");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch package");
    std::fs::write(
        dir.join("main.wrela"),
        r#"
use std::serialize::{Reader, Writer}

pub fn run() -> u32 {
    var text = String::new()
    text.push_str("")
    assert(text.is_empty())
    text.push_str("a")
    text.push_str("é🦀\0")
    let original = text.clone()
    for _i in 0..128 {
        text.push_str("β")
    }
    assert(text.len() == 264)
    assert(text.as_str().slice(from: 0, to: 8) == "aé🦀\0")
    for i in 0..128 {
        assert(text.as_str().slice(from: 8 + i * 2, to: 10 + i * 2) == "β")
    }
    assert(original == "aé🦀\0")
    text.clear()
    text.push_str(original.as_str().slice(from: 1, to: 7))
    text.push_str("")
    assert(text == "é🦀")
    let suffix = String::from(" τέλος")
    text.push_str(suffix)
    assert(suffix == " τέλος")
    assert(text == "é🦀 τέλος")
    var writer = Writer::new()
    writer.write_str("é🦀\0")
    writer.write_str("")
    let bytes = take writer.into_bytes()
    assert(bytes.len() == 15)
    assert(bytes[0] == 7 && bytes[1] == 0 && bytes[2] == 0 && bytes[3] == 0)
    for i in 0..7 {
        assert(bytes[i + 4] == "é🦀\0".byte(i))
    }
    for i in 11..15 {
        assert(bytes[i] == 0)
    }
    var reader = Reader::new(take bytes)
    assert(reader.read_string().unwrap() == "é🦀\0")
    assert(reader.read_string().unwrap() == "")
    assert(reader.at_end())
    text.len()
}

pub fn frame(time: f32, width: u32, height: u32) {}
"#,
    )
    .expect("write source");
    let built = wrela_tests::build(&dir).expect("build string append regression");
    let out = dir.join("build");
    built.write_to(&out).expect("write build");
    let mut host = CpuHost::load(&out).expect("load string append regression");
    assert_eq!(wrela_tests::one_u32(&mut host, "run", &[]), 17);
}

#[test]
fn frontend_perf_sort_preserves_comparator_calls_and_values() {
    let dir = wrela_tests::scratch_root(env!("CARGO_TARGET_TMPDIR")).join("frontend-perf-sort");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch package");
    std::fs::write(
        dir.join("main.wrela"),
        r#"
// The original element-at-a-time merge, through public Vec operations: a comparison's
// side effects must occur in the same order, including when either run runs out first.
fn original(v: mut Vec<u32>, before: fn(u32, u32) -> bool) {
    let n = v.len()
    var lo = 0
    while lo < n {
        let hi = min(lo + 16, n)
        for i in lo + 1..hi {
            var j = i
            while j > lo && before(v[j], v[j - 1]) {
                v.swap(j, j - 1)
                j -= 1
            }
        }
        lo = hi
    }
    var width = 16
    var room = v.clone()
    while width < n {
        var lo = 0
        while lo < n {
            let mid = min(lo + width, n)
            let hi = min(lo + 2 * width, n)
            var i = lo
            var j = mid
            for k in lo..hi {
                let left = i < mid && (j >= hi || !before(v[j], v[i]))
                if left {
                    room[k] = v[i]
                    i += 1
                } else {
                    room[k] = v[j]
                    j += 1
                }
            }
            lo = hi
        }
        for i in 0..n { v[i] = room[i] }
        width *= 2
    }
}

pub fn run() -> u32 {
    var runs = 0
    for n in [0, 1, 15, 16, 17, 31, 32, 33, 64, 65] {
        for kind in 0..4 {
            var v = Vec::from_fn(n, |i| match kind {
                0 => i,
                1 => n - i,
                2 => 7,
                _ => (i * 7919) % max(n, 1),
            })
            var expected = v.clone()
            var calls: Vec<(u32, u32)> = Vec::new()
            var expected_calls: Vec<(u32, u32)> = Vec::new()
            v.sort_by(|a, b| {
                calls.push((a, b))
                a % 7 < b % 7
            })
            original(mut expected, |a, b| {
                expected_calls.push((a, b))
                a % 7 < b % 7
            })
            assert(v == expected)
            assert(calls == expected_calls)
            runs += 1
        }
    }
    runs
}

pub fn frame(time: f32, width: u32, height: u32) {}
"#,
    )
    .expect("write sort regression");
    let built = wrela_tests::build(&dir).expect("build sort regression");
    let out = dir.join("build");
    built.write_to(&out).expect("write build");
    let mut host = CpuHost::load(&out).expect("load sort regression");
    assert_eq!(wrela_tests::one_u32(&mut host, "run", &[]), 40);
}
