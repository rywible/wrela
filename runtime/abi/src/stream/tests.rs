use super::*;

fn draw(uniforms: Vec<u8>) -> Command {
    Command::Draw {
        pipeline: 7,
        vertex_count: 3,
        instance_count: 1,
        uniforms,
    }
}

fn begin() -> Command {
    Command::BeginScreenPass {
        clear: [0.0, 0.25, 0.5, 1.0],
    }
}

/// One of each command, as one buffer.
fn sample() -> Vec<Command> {
    vec![
        begin(),
        draw((0..16).collect()),
        draw(Vec::new()),
        Command::Present,
    ]
}

/// Little-endian words, for writing expected bytes.
fn words(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

#[test]
fn the_header_is_wr_then_the_version() {
    assert_eq!(header(0), [0x57, 0x52, 0x00, 0x00]);
    assert_eq!(u32::from_le_bytes(header(0)), 0x0000_5257);
    assert_eq!(header(0x0102), [0x57, 0x52, 0x02, 0x01]);
    assert_eq!(Encoder::new().finish(), header(VERSION));
}

/// The exact bytes of each command: the vectors the TypeScript mirror tests against too.
#[test]
fn each_command_has_exactly_these_bytes() {
    let mut expected = header(0).to_vec();
    expected.extend(words(&[
        opcode::BEGIN_SCREEN_PASS,
        16,
        0.0f32.to_bits(),
        0.25f32.to_bits(),
        0.5f32.to_bits(),
        1.0f32.to_bits(),
    ]));
    expected.extend(words(&[opcode::DRAW, 12 + 16, 7, 3, 1]));
    expected.extend(0..16u8);
    expected.extend(words(&[opcode::DRAW, 12, 7, 3, 1]));
    expected.extend(words(&[opcode::PRESENT, 0]));
    assert_eq!(encode(&sample()).unwrap(), expected);
    assert_eq!(
        &expected[4..12],
        &[1, 0, 0, 0, 16, 0, 0, 0],
        "opcode, then payload length"
    );
}

#[test]
fn round_trips() {
    for commands in [
        vec![],
        vec![Command::Present],
        vec![begin()],
        vec![draw(vec![0; 4])],
        vec![draw(vec![0xff; 65536])],
        sample(),
        vec![
            Command::BeginScreenPass {
                clear: [-0.0, f32::MAX, f32::MIN_POSITIVE, -1e30],
            },
            Command::Draw {
                pipeline: u32::MAX,
                vertex_count: 0,
                instance_count: u32::MAX,
                uniforms: vec![1, 2, 3, 4, 5, 6, 7, 8],
            },
        ],
    ] {
        let bytes = encode(&commands).unwrap();
        assert_eq!(decode(&bytes).unwrap(), commands, "{commands:?}");
    }
}

#[test]
fn negative_zero_survives_bit_for_bit() {
    let bytes = encode(&[Command::BeginScreenPass {
        clear: [-0.0, 0.0, 0.0, 0.0],
    }])
    .unwrap();
    let Command::BeginScreenPass { clear } = &decode(&bytes).unwrap()[0] else {
        panic!("not a pass");
    };
    assert_eq!(clear[0].to_bits(), (-0.0f32).to_bits());
}

#[test]
fn the_encoder_refuses_what_the_decoder_would_reject() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for component in 0..4 {
            let mut clear = [0.0; 4];
            clear[component] = bad;
            let mut encoder = Encoder::new();
            let err = encoder
                .push(&Command::BeginScreenPass { clear })
                .unwrap_err();
            assert!(
                matches!(err, EncodeError::NonFiniteClear { component: c, .. } if c == component)
            );
            // A refused command leaves the buffer as it was.
            assert_eq!(encoder.bytes(), header(VERSION));
        }
    }
    for len in [1, 2, 3, 5, 17] {
        let mut encoder = Encoder::new();
        encoder.push(&Command::Present).unwrap();
        let before = encoder.bytes().to_vec();
        assert_eq!(
            encoder.push(&draw(vec![0; len])).unwrap_err(),
            EncodeError::UnalignedUniforms { len }
        );
        assert_eq!(encoder.bytes(), before);
    }
}

#[test]
fn a_header_alone_is_an_empty_buffer() {
    assert_eq!(decode(&header(0)).unwrap(), vec![]);
}

#[test]
fn short_or_foreign_headers_are_rejected() {
    for len in 0..4 {
        assert_eq!(
            decode(&header(0)[..len]),
            Err(DecodeError::MissingHeader { len })
        );
    }
    assert_eq!(
        decode(b"RW\0\0"),
        Err(DecodeError::BadMagic { found: *b"RW" })
    );
    assert_eq!(
        decode(&[0, 0, 0, 0]),
        Err(DecodeError::BadMagic { found: [0, 0] })
    );
    assert_eq!(
        decode(&header(1)),
        Err(DecodeError::UnsupportedVersion { found: 1 })
    );
    assert_eq!(
        decode(&header(0xffff)),
        Err(DecodeError::UnsupportedVersion { found: 0xffff })
    );
}

#[test]
fn every_truncation_is_an_error_or_a_whole_prefix_of_commands() {
    let commands = sample();
    let bytes = encode(&commands).unwrap();
    // Where each command ends: a cut exactly there decodes to the commands before it.
    let mut ends = vec![HEADER_LEN];
    let mut encoder = Encoder::new();
    for command in &commands {
        encoder.push(command).unwrap();
        ends.push(encoder.bytes().len());
    }
    for cut in 0..bytes.len() {
        match ends.iter().position(|&end| end == cut) {
            Some(n) => assert_eq!(decode(&bytes[..cut]).unwrap(), commands[..n], "cut {cut}"),
            None => assert!(decode(&bytes[..cut]).is_err(), "cut {cut} decoded"),
        }
    }
}

#[test]
fn a_command_header_cut_short_is_truncated() {
    let mut bytes = header(0).to_vec();
    bytes.extend(words(&[opcode::PRESENT]));
    assert_eq!(
        decode(&bytes),
        Err(DecodeError::TruncatedCommand {
            offset: 4,
            remaining: 4
        })
    );
}

#[test]
fn payload_lengths_must_be_aligned_and_in_bounds() {
    for length in [1, 2, 3, 13, 15] {
        let mut bytes = header(0).to_vec();
        bytes.extend(words(&[opcode::DRAW, length, 0, 0, 0, 0]));
        assert_eq!(
            decode(&bytes),
            Err(DecodeError::UnalignedLength {
                offset: 4,
                opcode: opcode::DRAW,
                length
            })
        );
    }
    for length in [4, 16, 0xffff_fffc] {
        let mut bytes = header(0).to_vec();
        bytes.extend(words(&[opcode::PRESENT, 0, opcode::DRAW, length]));
        assert_eq!(
            decode(&bytes),
            Err(DecodeError::PayloadPastEnd {
                offset: 12,
                opcode: opcode::DRAW,
                length,
                available: 0
            })
        );
    }
}

#[test]
fn unknown_opcodes_are_errors_never_skipped() {
    for opcode in [0, 4, 5, 255, 0x0100, u32::MAX] {
        let mut bytes = header(0).to_vec();
        bytes.extend(words(&[opcode, 0]));
        bytes.extend(words(&[opcode::PRESENT, 0]));
        assert_eq!(
            decode(&bytes),
            Err(DecodeError::UnknownOpcode { offset: 4, opcode })
        );
    }
}

#[test]
fn each_opcode_takes_only_its_payload_lengths() {
    let cases: &[(u32, &[u32], &str)] = &[
        (opcode::BEGIN_SCREEN_PASS, &[0, 4, 12, 20, 32], "exactly 16"),
        (opcode::DRAW, &[0, 4, 8], "at least 12"),
        (opcode::PRESENT, &[4, 8, 16], "exactly 0"),
    ];
    for &(opcode, lengths, expected) in cases {
        for &length in lengths {
            let mut bytes = header(0).to_vec();
            bytes.extend(words(&[opcode, length]));
            bytes.extend(vec![0; length as usize]);
            assert_eq!(
                decode(&bytes),
                Err(DecodeError::WrongLength {
                    offset: 4,
                    opcode,
                    length,
                    expected
                }),
                "opcode {opcode}, length {length}"
            );
        }
    }
}

#[test]
fn a_non_finite_clear_is_rejected_at_its_command() {
    for bad in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(0x7f80_0001),
    ] {
        for component in 0..4 {
            let mut clear = [0u32; 4];
            clear[component] = bad.to_bits();
            let mut bytes = encode(&[Command::Present]).unwrap();
            bytes.extend(words(&[opcode::BEGIN_SCREEN_PASS, 16]));
            bytes.extend(words(&clear));
            assert_eq!(
                decode(&bytes),
                Err(DecodeError::NonFiniteClear {
                    offset: 12,
                    component
                })
            );
        }
    }
}

/// Every opcode value in every command slot, and every byte value at every position of a valid
/// buffer: the decoder returns, never panics, and accepts only what re-encodes to the same bytes.
#[test]
fn single_byte_corruptions_never_panic_and_never_decode_to_other_bytes() {
    let bytes = encode(&sample()).unwrap();
    for at in 0..bytes.len() {
        for value in [0u8, 1, 2, 3, 4, 0x7f, 0x80, 0xff] {
            let mut corrupt = bytes.clone();
            corrupt[at] = value;
            if let Ok(commands) = decode(&corrupt) {
                assert_eq!(encode(&commands).unwrap(), corrupt, "byte {at} = {value}");
            }
        }
    }
}

#[test]
fn errors_report_their_offset_and_read_as_messages() {
    let errors = [
        DecodeError::MissingHeader { len: 2 },
        DecodeError::BadMagic { found: *b"XY" },
        DecodeError::UnsupportedVersion { found: 3 },
        DecodeError::TruncatedCommand {
            offset: 12,
            remaining: 4,
        },
        DecodeError::UnalignedLength {
            offset: 12,
            opcode: 2,
            length: 3,
        },
        DecodeError::PayloadPastEnd {
            offset: 12,
            opcode: 9,
            length: 8,
            available: 4,
        },
        DecodeError::UnknownOpcode {
            offset: 12,
            opcode: 9,
        },
        DecodeError::WrongLength {
            offset: 12,
            opcode: 3,
            length: 4,
            expected: "exactly 0",
        },
        DecodeError::NonFiniteClear {
            offset: 12,
            component: 1,
        },
    ];
    let offsets: Vec<usize> = errors.iter().map(DecodeError::offset).collect();
    assert_eq!(offsets, [0, 0, 2, 12, 12, 12, 12, 12, 12]);
    for error in errors {
        let message = error.to_string();
        assert!(!message.ends_with('.'), "{message}");
        assert!(!message.starts_with(char::is_uppercase), "{message}");
    }
    assert_eq!(
        DecodeError::WrongLength {
            offset: 4,
            opcode: 3,
            length: 4,
            expected: "exactly 0"
        }
        .to_string(),
        "at byte 4: PRESENT has a 4-byte payload; it takes exactly 0 bytes"
    );
    assert_eq!(
        DecodeError::UnknownOpcode {
            offset: 4,
            opcode: 0
        }
        .to_string(),
        "at byte 4: unknown opcode 0"
    );
}

#[test]
fn a_frame_is_one_pass_some_draws_and_present() {
    for draws in 0..3 {
        let mut check = FrameCheck::new();
        assert_eq!(check.command(&begin()), Ok(()));
        for _ in 0..draws {
            assert_eq!(check.command(&draw(vec![])), Ok(()));
        }
        assert!(!check.is_presented());
        assert_eq!(check.command(&Command::Present), Ok(()));
        assert!(check.is_presented());
        assert_eq!(check.finish(), Ok(()));
    }
}

#[test]
fn frames_that_break_the_rules_are_errors() {
    let run = |commands: &[Command]| -> Result<(), SequenceError> {
        let mut check = FrameCheck::new();
        for command in commands {
            check.command(command)?;
        }
        check.finish()
    };
    assert_eq!(run(&[]), Err(SequenceError::NotPresented));
    assert_eq!(run(&[begin()]), Err(SequenceError::NotPresented));
    assert_eq!(
        run(&[begin(), draw(vec![])]),
        Err(SequenceError::NotPresented)
    );
    assert_eq!(run(&[draw(vec![])]), Err(SequenceError::DrawOutsidePass));
    assert_eq!(
        run(&[Command::Present]),
        Err(SequenceError::PresentWithoutPass)
    );
    assert_eq!(
        run(&[begin(), begin()]),
        Err(SequenceError::SecondScreenPass)
    );
    for after in [begin(), draw(vec![]), Command::Present] {
        let name = after.name();
        assert_eq!(
            run(&[begin(), Command::Present, after]),
            Err(SequenceError::AfterPresent { command: name })
        );
    }
}

#[test]
fn a_rejected_command_leaves_the_check_unchanged() {
    let mut check = FrameCheck::new();
    check.command(&begin()).unwrap();
    let before = check;
    assert!(check.command(&begin()).is_err());
    assert_eq!(check, before);
    assert_eq!(check.command(&Command::Present), Ok(()));
}

#[test]
fn names_match_the_spec() {
    assert_eq!(begin().name(), "BEGIN_SCREEN_PASS");
    assert_eq!(draw(vec![]).name(), "DRAW");
    assert_eq!(Command::Present.name(), "PRESENT");
    assert_eq!(opcode_name(0), None);
    assert_eq!(opcode_name(4), None);
    for message in [
        SequenceError::DrawOutsidePass.to_string(),
        SequenceError::NotPresented.to_string(),
        EncodeError::UnalignedUniforms { len: 3 }.to_string(),
    ] {
        assert!(!message.ends_with('.'), "{message}");
    }
}

/// The spec's own description of the stream, checked against the constants here.
#[test]
fn the_spec_document_agrees() {
    let spec = include_str!("../../../command-stream.md");
    assert!(spec.starts_with(&format!("# wrela runtime contract, version {VERSION}\n")));
    for (opcode, name) in [
        (opcode::BEGIN_SCREEN_PASS, "BEGIN_SCREEN_PASS"),
        (opcode::DRAW, "DRAW"),
        (opcode::PRESENT, "PRESENT"),
    ] {
        let row = format!("| {opcode} | `{name}` |");
        assert!(spec.contains(&row), "command-stream.md has no row {row:?}");
    }
    assert!(spec.contains("0x00005257"));
}

/// A reference stream for the hosts and the TypeScript mirror: 60 test-mode frames shaped like
/// first light's, each a pass cleared to opaque black, one 3-vertex draw of pipeline 0 whose
/// 16-byte uniform is `[width, height, time, 0]` as f32s (the last word is padding), and a
/// present, each command in its own buffer.
#[test]
fn reference_stream_hash() {
    use crate::{StateHash, test_mode};
    let mut hash = StateHash::new();
    let mut frame_59_draw = Vec::new();
    for i in 0..test_mode::FRAMES {
        let mut uniforms = Vec::new();
        for word in [
            (test_mode::WIDTH as f32).to_bits(),
            (test_mode::HEIGHT as f32).to_bits(),
            test_mode::time(i).to_bits(),
            0,
        ] {
            uniforms.extend(word.to_le_bytes());
        }
        let mut check = FrameCheck::new();
        for command in [
            Command::BeginScreenPass {
                clear: [0.0, 0.0, 0.0, 1.0],
            },
            Command::Draw {
                pipeline: 0,
                vertex_count: 3,
                instance_count: 1,
                uniforms,
            },
            Command::Present,
        ] {
            check.command(&command).unwrap();
            let buffer = encode(std::slice::from_ref(&command)).unwrap();
            if i == 59 && command.opcode() == opcode::DRAW {
                frame_59_draw.clone_from(&buffer);
            }
            hash.update(&buffer);
        }
        check.finish().unwrap();
    }
    let hex: String = frame_59_draw.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        hex,
        "57520000020000001c0000000000000003000000010000000000f04400008744bcbb7b3f00000000"
    );
    assert_eq!(hash.hex(), "ee6a915168bafdc0");
}

/// A frame's draws draw at most `MAX_FRAME_VERTICES` vertices in all, counting each draw's
/// vertices times its instances. The browser runtime's stream tests check the same cases.
#[test]
fn a_frame_draws_at_most_its_vertex_budget() {
    let counted = |vertex_count: u32, instance_count: u32| Command::Draw {
        pipeline: 0,
        vertex_count,
        instance_count,
        uniforms: vec![],
    };
    let run = |draws: &[(u32, u32)]| -> Result<(), SequenceError> {
        let mut check = FrameCheck::new();
        check.command(&begin())?;
        for &(v, i) in draws {
            check.command(&counted(v, i))?;
        }
        check.command(&Command::Present)?;
        check.finish()
    };
    assert_eq!(MAX_FRAME_VERTICES, 1_048_576);
    assert_eq!(run(&[(1 << 20, 1)]), Ok(()));
    assert_eq!(run(&[(1 << 10, 1 << 10)]), Ok(()));
    assert_eq!(run(&[(3, 0), (0, u32::MAX), (1 << 20, 1)]), Ok(()));
    assert_eq!(
        run(&[(1 << 20, 1), (1, 1)]),
        Err(SequenceError::TooManyVertices {
            vertices: 1_048_577
        })
    );
    assert_eq!(
        run(&[(u32::MAX, u32::MAX)]),
        Err(SequenceError::TooManyVertices {
            vertices: 18_446_744_065_119_617_025
        })
    );
    assert_eq!(
        run(&[(3, 1 << 19)]),
        Err(SequenceError::TooManyVertices {
            vertices: 1_572_864
        })
    );
    assert_eq!(
        SequenceError::TooManyVertices {
            vertices: 1_572_864
        }
        .to_string(),
        "this frame's DRAWs draw 1572864 vertices (vertex_count × instance_count, summed), over \
         the 1048576 a frame may draw"
    );
    // A rejected draw doesn't count.
    let mut check = FrameCheck::new();
    check.command(&begin()).unwrap();
    assert!(check.command(&counted(u32::MAX, 2)).is_err());
    assert_eq!(check.command(&counted(1 << 20, 1)), Ok(()));
}
