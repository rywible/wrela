;; A hand-written stand-in for the compiled first-light program (examples/first-light/main.wrela),
;; so the runtime can be tested without the compiler. Each frame submits the three commands
;; first light records, each in its own buffer, as the S1 WASM back end does:
;; BEGIN_SCREEN_PASS cleared to opaque black, a 3-vertex DRAW of pipeline 0 whose uniform is
;; Scene { resolution: (width, height), time } (16 bytes: 4 of them padding, zeroed), and PRESENT.
;; Over the 60 test-mode frames the state hash is wrela-abi's reference, ee6a915168bafdc0.
(module
  (import "wrela" "submit" (func $submit (param i32 i32)))
  (memory (export "memory") 1)

  ;; At 0, 28 bytes: header "WR" + version 0, BEGIN_SCREEN_PASS (1), 16-byte payload, clear (0, 0, 0, 1).
  (data (i32.const 0)
    "WR\00\00" "\01\00\00\00" "\10\00\00\00"
    "\00\00\00\00" "\00\00\00\00" "\00\00\00\00" "\00\00\80\3f")

  ;; At 32, 40 bytes: header, DRAW (2), 28-byte payload: pipeline 0, 3 vertices, 1 instance, then
  ;; the 16-byte uniform at 56, written each frame.
  (data (i32.const 32)
    "WR\00\00" "\02\00\00\00" "\1c\00\00\00"
    "\00\00\00\00" "\03\00\00\00" "\01\00\00\00")

  ;; At 80, 12 bytes: header, PRESENT (3), empty payload.
  (data (i32.const 80) "WR\00\00" "\03\00\00\00" "\00\00\00\00")

  (func (export "frame") (param $time f32) (param $width i32) (param $height i32)
    (call $submit (i32.const 0) (i32.const 28))
    (f32.store (i32.const 56) (f32.convert_i32_u (local.get $width)))
    (f32.store (i32.const 60) (f32.convert_i32_u (local.get $height)))
    (f32.store (i32.const 64) (local.get $time))
    (i32.store (i32.const 68) (i32.const 0))
    (call $submit (i32.const 32) (i32.const 40))
    (call $submit (i32.const 80) (i32.const 12))))
