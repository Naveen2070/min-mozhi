# Min-Mozhi: Modernization & Advanced Language Plan

This document outlines the conceptual roadmap for elevating Min-Mozhi into the "Ultimate Modern HDL". It combines the best of several languages:

- the strict mathematical safety of **Rust**,
- the developer ergonomics of **TypeScript** and **Kotlin**,
- the concurrency paradigms of **Go**,
- and the testing architecture of **C#**.

By doing so, Min-Mozhi aims to bridge the gap between software engineering and physical silicon design.

---

## 1. Safety by Construction (Inspired by Rust)

The core philosophy of Rust is catching bugs at compile time. In hardware, runtime bugs are fatal and cost millions of dollars in silicon respins.

### 1.1 The Hardware Borrow Checker (Spatial & Temporal Borrowing)

- **Explanation:** In Rust, you cannot have two mutable references to the same data at the same time. In hardware, you cannot have two concurrent state machines trying to drive the same physical resource (like a shared Arithmetic Logic Unit) at the exact same clock cycle.
- **Mechanism:** The compiler builds an execution graph and statically analyzes resource usage.
- **Example:**

  ```mimz
  // A shared hardware resource
  let shared_alu = ALU()

  on rise(clk) {
      // FSM 1
      match (fsm_1_state) {
          CALC => { shared_alu.in <- data_A } // Borrowing ALU
      }

      // FSM 2
      match (fsm_2_state) {
          // ❌ COMPILE ERROR: Temporal Borrow Violation.
          // fsm_1_state == CALC and fsm_2_state == CALC can happen simultaneously!
          CALC => { shared_alu.in <- data_B }
      }
  }
  ```

### 1.2 Clock Domain Checker (Hardware Thread-Safety)

- **Explanation:** Passing a signal from one clock domain to another without proper synchronization (CDC) causes metastability, where the chip enters an undefined electrical state. This is the hardware equivalent of a software data race.
- **Example:**

  ```mimz
  clock clk_100mhz
  clock clk_uart  // 115200 Hz

  reg fast_data: bits[8] @ clk_100mhz
  reg slow_data: bits[8] @ clk_uart

  on rise(clk_uart) {
      // ❌ COMPILE ERROR: Clock domain violation!
      slow_data <- fast_data

      // ✅ OK: Must use an explicit synchronizer from the standard library
      slow_data <- sync.double_flop(fast_data, clk_100mhz, clk_uart)
  }
  ```

  > `sync` is now an active keyword for the unrelated `sync loop` construct
  > (spec/03-keywords-trilingual.md v0.2.22) — the two are dual-purposed,
  > not conflicting, since the parser disambiguates by the token after
  > `sync` (`loop`/`suzhal`/`சுழல்` vs `.`). See that changelog entry before
  > designing this CDC construct's real grammar.

### 1.3 Affine Types (Preventing Dropped Data)

- **Explanation:** In an AXI-Stream or pipeline, dropping valid data causes deadlocks. If a stream returns `valid`, it is an "Affine Type" (it must be consumed or explicitly dropped).
- **Example:**

  ```mimz
  // Function returns a pipeline token
  let token = fetch_instruction()

  // ❌ COMPILE ERROR: Token must be routed to a register or explicitly dumped!
  // Ignoring the token causes a pipeline leak.
  ```

### 1.4 State Machine Exhaustiveness

- **Explanation:** A common hardware vulnerability is an FSM entering a dead state due to a radiation-induced bit flip and getting stuck forever.
- **Example:**

  ```mimz
  match (current_state) {
      IDLE => ...,
      RUNNING => ...,
      // ❌ COMPILE ERROR: Missing fallback case!
      // Must include `_ => system_fault()` or `_ => IDLE` to handle invalid states.
  }
  ```

### 1.5 Hardware `unsafe {}` Blocks

- **Explanation:** Sometimes hardware engineers need to intentionally break the rules (e.g., creating a combinational loop for a physical ring oscillator, or creating a custom latch). The compiler bans latches and combinational loops by default, but allows them inside explicit `unsafe` blocks so the risk is contained and documented.
- **Example:**

  ```mimz
  unsafe {
      // The compiler turns off the cycle-checker and latch-checker here.
      // If the chip fails, the engineer knows exactly where to look.
      ring_oscillator = !ring_oscillator
  }
  ```

### 1.6 Explicit "Pulse" Lifetimes

- **Explanation:** In software, variables live until they go out of scope. In hardware, a variable might only be valid for a single clock cycle (a pulse). The `pulse` modifier mathematically enforces a lifetime of exactly 1 cycle.
- **Example:**

  ```mimz
  wire valid_flag: pulse

  on rise(clk) {
      if (condition) valid_flag = 1
  }

  on rise(clk) {
      // ❌ COMPILE ERROR: 'valid_flag' expired on the previous cycle.
      // Must store it in a register if you want to read it later.
      if (valid_flag) do_something()
  }
  ```

---

## 2. Developer Ergonomics (Inspired by TypeScript & Kotlin)

### 2.1 High-Z / Null Safety (Kotlin)

- **Explanation:** A "floating" wire (High-Z) is the hardware equivalent of a Null Pointer Exception. It means the wire is not connected to any logic gate, which destroys signal integrity.
- **Example:**

  ```mimz
  wire a: bits[8]     // Must ALWAYS be driven continuously.
  wire b: bits[8]?    // Optional (?): Synthesizes to a Tri-State Buffer.

  // ❌ COMPILE ERROR: Cannot read a floating wire directly.
  let sum = a + b

  // ✅ OK: Must safely unwrap the floating wire, providing a default value.
  let safe_b = b ?? 0
  let sum = a + safe_b
  ```

### 2.2 Extension Functions (Kotlin)

- **Explanation:** Instead of writing messy wrapper modules for simple bitwise operations, allow users to attach functions to primitive types.
- **Example:**

  ```mimz
  // Define an extension to count active bits
  fn bits[N].count_ones() -> bits[log2(N)] {
      // Internal recursive logic...
  }

  let sensor_bus: bits[32] = get_inputs()

  // Synthesizes a parallel adder tree natively, but reads beautifully!
  let active_count = sensor_bus.count_ones()
  ```

### 2.3 Local Scope & Type Inference (TypeScript)

- **Explanation:** Prevent Verilog's catastrophic truncation bugs by inferring the required physical bit-width automatically.
- **Example:**

  ```mimz
  let a: bits[8] = 255
  let b: bits[8] = 2

  // The compiler infers 'sum' MUST be 9 bits to prevent overflow.
  let sum = a + b
  ```

### 2.4 Interface Destructuring (TypeScript)

- **Explanation:** Cleanly unpack complex hardware buses.
- **Example:**

  ```mimz
  interface MemoryBus(WIDTH) {
      valid: bit,
      data: bits[WIDTH]
  }

  let mem_response: MemoryBus(32) = memory.read()

  // Physically unpack the 33-bit bus into two distinct local wires
  let { valid, data } = mem_response
  ```

### 2.5 Array `.map()`, `.filter()`, `.reduce()` (JavaScript)

- **Explanation:** Eliminates ugly, error-prone Verilog `generate for` loops by using functional array methods to stamp out parallel hardware gates at compile time.
- **Example:**

  ```mimz
  let raw_signals = [sig1, sig2, sig3, sig4]

  // Physically synthesizes 4 parallel NOT gates
  let inverted_signals = raw_signals.map(not)

  // Synthesizes a massive 4-input OR gate tree
  let any_active = inverted_signals.reduce(or)
  ```

### 2.6 Hardware "Closures" / Parameterized Generators (JavaScript)

- **Explanation:** In hardware, you can't capture runtime data, but you can capture compile-time parameters to generate custom logic blocks dynamically, acting like closures.
- **Example:**

  ```mimz
  // A compile-time hardware generator
  const make_delay_line = (cycles: int) => {
      return module {
          in data: bit
          out delayed: bit
          // Physically stamps out N flip-flops based on 'cycles'
          delayed = shift_reg(data, depth=cycles)
      }
  }

  // Instantiate physical modules using the closure
  let delay_5 = make_delay_line(5)
  let delay_10 = make_delay_line(10)
  ```

### 2.7 Smart Casts & Tagged Unions (Kotlin / C#)

- **Explanation:** Verilog handles multiple packet types by manually packing/unpacking bits (`[63:0]`). Tagged Unions combined with exhaustive matching act as "Smart Casts", automatically extracting data payload.
- **Example:**

  ```mimz
  enum Packet {
      Read(addr: bits[32]),
      Write(addr: bits[32], data: bits[32])
  }

  // The bus physically synthesizes to 65 bits (1 bit tag + 64 bits data)
  let bus: Packet = receive()

  match (bus) {
      Read(addr) => read_memory(addr), // Smart cast extracts 'addr'
      Write(addr, data) => write_memory(addr, data)
  }
  ```

### 2.8 Properties (Getters / Setters from C#)

- **Explanation:** In Verilog, managing who is allowed to drive a signal vs who is allowed to read it is messy. Min-Mozhi introduces strict `in/out` scoping using C#-style properties, eliminating accidental multi-driver errors.
- **Example:**

  ```mimz
  module Counter {
      // Any external module can READ this value,
      // but only the internal Counter logic can SET it.
      out count: bits[8] { get, private set }

      on rise(clk) {
          count <- count + 1 // Internal logic is allowed to drive it
      }
  }
  ```

### 2.9 Implicit Interfaces / Duck Typing (Go)

- **Explanation:** If a module has the right input/output ports, it can be plugged into a bus interface without needing to explicitly declare `implements InterfaceName`. This structural subtyping makes hardware composition fluid and agile.
- **Example:**

  ```mimz
  interface HasUART {
      out tx: bit
      in  rx: bit
  }

  module SensorData {
      // Matches the shape of HasUART perfectly, but doesn't explicitly declare it
      out tx: bit
      in  rx: bit
  }

  // The compiler accepts this automatically!
  let bus: HasUART = SensorData()
  ```

---

## 3. Concurrency & Control Flow (Inspired by Go & C#)

### 3.1 Hardware Channels (Go)

- **Explanation:** Abstract away the nightmare of `valid/ready/stall` handshake signals.
- **Example:**

  ```mimz
  // Automatically creates valid/ready/data wires
  chan data_bus: bits[32]

  // Module A (Sends data, stalls if receiver is busy)
  data_bus <- 0xFF

  // Module B (Waits until data is valid)
  let payload = <-data_bus
  ```

### 3.2 The `defer` Keyword (Go)

- **Explanation:** Guarantee that control signals are cleaned up when an FSM transitions.
- **Example:**

  ```mimz
  match (state) {
      WRITING => {
          mem_write_enable = 1

          // Guarantees this wire is pulled LOW when the state transitions!
          defer mem_write_enable = 0

          if (done) state <- IDLE
      }
  }
  ```

### 3.3 Async/Await for Testbenches (C#)

- **Explanation:** Eliminate archaic `@(posedge clk)` timing loops in test simulations.
- **Example:**

  ```mimz
  test "UART Boot Sequence" async {
      reset <- 1
      await clk.cycles(5) // Suspends execution for 5 cycles
      reset <- 0

      // Await a hardware response
      let response = await uart.read_byte()
  }
  ```

---

## 4. The Two-Tiered Hardware Error System

Because hardware has no Operating System, a `try/catch` block cannot "unwind a call stack" (there is no stack, only physical wires). Therefore, Min-Mozhi strictly rejects `try/catch` and Go-style `if err != nil` patterns.

Instead, error handling is split into Simulation logic and Physical Silicon logic.

### 4.1 Simulation-Only Errors (SystemVerilog Style)

These tools are for the testbench and do **not** synthesize into physical gates.

- **Mechanism:** Use the `sim::` namespace.
- **Example:**

  ```mimz
  if (address >= MEM_DEPTH) {
      sim::fatal("Address Out of Bounds!") // Halts simulation instantly
  }
  ```

### 4.2 Tier 1: Recoverable Silicon Errors (Rust's `Result` & `match`)

For hardware errors the chip must recover from, we use Rust's `Result` type rather than Go's tuple return `(data, err)`.

- **Why not Go's `(data, err)`?** A developer can easily forget to check `if (err)`, inadvertently feeding corrupt `data` into downstream logic.
- **Why Rust's `Result`?** The `match` statement mathematically forces the engineer to handle the error before extracting the `data`. It is **Safe by Construction**.
- **Example:**

  ```mimz
  let status = decode_packet()

  match (status) {
      Ok(payload) => process(payload), // 'payload' only exists here!
      Err(code) => send_nack(code)     // Forced to handle the error
  }
  ```

### 4.3 Tier 2: Unrecoverable Silicon Errors (Hardware Panics)

For catastrophic faults (e.g., impossible FSM states, division by zero).

- **Mechanism:** The `system_fault(code)` keyword.
- **Dual Compilation:**
  - **In Simulation (`mimz sim`):** Translates to a SystemVerilog `$fatal` to instantly halt the testbench.
  - **In Physical Synthesis (`mimz compile`):** Synthesizes the **Recovery Mode Network**:
    1. Instantly halts the chip's internal logic clocks.
    2. Forces all output pins to a predefined "Safe State" (e.g., disabling motors/lasers).
    3. Asserts a physical `FAULT_OUT` pin to alert the host CPU.
    4. The chip ignores all inputs until a physical cold reset.

---

## 5. The Ultimate Synthesis: Secure Router Example

Here is what all these concepts look like when combined into a single, cohesive Min-Mozhi module:

```mimz
// 1. TypeScript Style: Interfaces
interface Packet {
    dest_ip: bits[32]
    payload: bits[128]
}

// 2. Kotlin Style: Tagged Unions
enum RouterStatus {
    Active,
    RecoverableError(code: bits[8]),
    FatalCorruption
}

// 3. Rust Safety: Strict Clock Domains
module SecureRouter(MAX_RETRIES: int = 3) {
    clock clk_core

    // 4. Go Style: Channels (Automatic valid/ready generation)
    in  rx_stream: chan Packet @ clk_core
    out tx_stream: chan Packet @ clk_core

    // 5. Kotlin Style: Null-Safety / High-Z prevention
    out status_led: bit? = null

    reg retries: bits[4] = 0

    on rise(clk_core) {
        status_led = 1 // Turn on LED

        // 6. Go Style: Blocking channel read
        let incoming_data = <-rx_stream

        // 7. Tier 1 Error: Recoverable Runtime Result
        let validation_result: Result<Packet, RouterStatus> = validate(incoming_data)

        // 8. Rust Style: Exhaustive Pattern Matching
        match (validation_result) {
            Ok(packet) => {
                tx_stream <- packet  // Forward out
                retries <- 0
            },

            Err(RouterStatus::RecoverableError(code)) => {
                sim::warn("Packet corrupted, retrying...")
                retries <- retries +% 1

                // 9. Tier 2 Error: The System Fault / Panic
                if (retries > MAX_RETRIES) {
                    // Halts chip, stops clock, outputs go to Safe State
                    system_fault(RouterStatus::FatalCorruption)
                }
            },

            _ => { retries <- 0 } // Compiler enforces exhaustion
        }
    }
}
```

---

## 6. Advanced Performance & Verification

Hardware engineering is ultimately constrained by clock speed (Fmax), security (silicon data leaks), and verification time. Min-Mozhi tackles these natively.

### 6.1 Optimized for Speed: Automatic Retiming (The `pipeline` Block)

- **Explanation:** To hit high clock frequencies, engineers must manually break math equations into chunks across multiple flip-flops. Min-Mozhi automates this. You write the logic, and the compiler inserts `N` registers to automatically balance the logic delay.
- **Example:**

  ```mimz
  // The compiler automatically figures out where to place
  // the 3 registers to achieve the highest possible clock speed!
  let result = pipeline(stages = 3) {
      (a * b) + (c * d) - (e / f)
  }
  ```

### 6.2 Hardware Security: Information Flow Tracking (Taint Analysis)

- **Explanation:** Hardware leaks can expose cryptographic keys. By marking a data type as `secret`, the compiler statically proves that the data can NEVER flow into a public output pin or non-secret register unless explicitly passed through a cryptography module.
- **Example:**

  ```mimz
  let root_key: secret bits[256] = get_fuse_data()
  out tx_pin: bit

  // ❌ COMPILE ERROR: Data Leakage Detected!
  // You cannot route a 'secret' wire to a public 'out' pin.
  tx_pin <- root_key[0]

  // ✅ OK: Data has been cryptographically transformed
  let ciphertext = aes_encrypt(root_key, payload)
  tx_pin <- ciphertext[0]
  ```

### 6.3 Next-Gen Verification: Built-in Formal Proofs

- **Explanation:** Writing randomized testbenches takes up 70% of a hardware engineer's time. Instead, Min-Mozhi allows you to write mathematical `properties`. The compiler uses an SMT Solver (like Z3) during compilation to mathematically prove that the rule can _never_ be broken by any infinite combination of inputs.
- **Example:**

  ```mimz
  module Arbiter {
      out grant_a: bit
      out grant_b: bit

      // Proves this is true for ALL possible inputs.
      // If there is a flaw, it generates the exact waveform to prove you wrong.
      prove strictly_mutually_exclusive {
          !(grant_a == 1 && grant_b == 1)
      }
  }
  ```

### 6.4 Time-Travel Debugging (Simulation Tooling)

- **Explanation:** When a standard Verilog simulation fails, engineers must dig through massive, visually noisy waveform files (VCDs) to find the root cause. Min-Mozhi's `mimz sim` tool treats simulations like a modern software debugger.
- **Mechanism:** If an `assert` or `prove` statement fails, the simulator pauses. In the terminal, the engineer can "step backwards" in time cycle-by-cycle (`step back`), instantly seeing the exact wire state that caused the cascading failure, drastically reducing debug time.

---

## 7. Feasibility triage (2026-06-12)

Reviewed against the compiler as it exists today (522 tests, checker passes 1–5),
what comparable HDLs (Chisel, Bluespec, SpinalHDL, Filament, SecVerilog) learned
building the same features, and the project constitution. Phase 1 scope is
unchanged by this document.

**Re-triaged same day under the v0.3 constitution** (spec/01: modern-secure-HDL
now co-primary with education; tie-breakers honesty > safety > **security** >
readability/DX > speed > brevity > Tamil idiom).

The goal shift promoted three items out of Tier 4 (see the Decision in `docs/log/2026-06-12.md`):

- `secret` taint,
- the `system_fault` fault network v1,
- and the `?` valid-bundle re-targeting.

---

## 8. Future Expansion & Cross-Language Inspirations (New Additions)(2026-06-12)

To further solidify Min-Mozhi's position as the "Ultimate Modern HDL," here are newly proposed, feasible ideas adopted from other modern languages. These focus heavily on three things:

- our educational mission,
- developer experience (DX),
- and robustness for digital signal processing (DSP).

### 8.1 Friendly, Didactic Compiler Errors (Inspired by Elm)

- **Explanation:** Elm is famous for having the best compiler error messages in the world. Instead of just pointing out an error, it explains the context, _why_ it's mathematically wrong, and suggests the exact fix.
- **Feasibility:** High (Tier 2/3). Since Min-Mozhi targets students, moving beyond standard Rust-style `ariadne`/`miette` spans to full "Teaching Errors" with hardware diagrams in ASCII.
- **Example:**

  ```text
  E0108: Multiple Drivers Detected
  You are trying to drive 'status_led' from two different places:

  12 |   on rise(clk_a) { status_led = 1 }
  24 |   on rise(clk_b) { status_led = 0 }

  Hardware doesn't allow two separate logic blocks to control the same physical wire simultaneously (it causes a short circuit!).
  Hint: Try creating a multiplexer (match/if) inside a single clock domain.
  ```

### 8.2 Contracts & Invariants at the Boundary (Inspired by Ada/SPARK & Kotlin)

- **Explanation:** Moving `prove` assertions to the public interface. A module can declare preconditions (`requires`) that the _caller_ must statically satisfy, and postconditions (`ensures`) it promises.
- **Feasibility:** Medium (Tier 3, relies on the `prove` / SymbiYosys integration).
- **Example:**

  ```mimz
  module Divider {
      in  numerator: bits[16]
      in  denominator: bits[16]
      out quotient: bits[16]

      // The caller MUST prove 'denominator' is never 0 before compiling.
      requires { denominator != 0 }
  }
  ```

### 8.3 Native Fixed-Point Arithmetic (Inspired by Julia & MATLAB)

- **Explanation:** Floating-point math is too expensive for most FPGAs, so hardware engineers use fixed-point arithmetic (e.g., 16 bits: 8 integer bits, 8 fractional bits). Doing this manually in Verilog requires error-prone bit-shifting.
- **Feasibility:** High (Tier 3). It translates to standard integer adders/multipliers but the compiler automatically aligns the radix points.
- **Example:**

  ```mimz
  // 16 total bits, 8 bits fractional
  let sensor_val: fixed[16, 8] = 1.5
  let gain: fixed[16, 8] = 2.0

  // The compiler aligns the fractional bits and prevents truncation!
  let output = sensor_val * gain
  ```

### 8.4 Compile-Time Logic execution / `$comptime` (Inspired by Zig & V)

- **Explanation:** Replacing the macro-preprocessor with actual language execution. Instead of `generate for`, you run actual Min-Mozhi code at compile-time to stamp out hardware.
- **Feasibility:** Medium. Extends the existing `repeat` functionality into a true compile-time interpreter.
- **Example:**

  ```mimz
  $if (DEBUG_MODE) {
      // This register physically exists ONLY if DEBUG_MODE is true.
      reg debug_counter: bits[32] = 0
  }
  ```

### 8.5 Interactive Hardware REPL (Inspired by Python & Swift Playgrounds)

- **Explanation:** A read-eval-print loop (REPL) where a student can define an expression or logic gate, flip inputs interactively, and see the combinational logic evaluate instantly in the terminal.
- **Feasibility:** Medium (Phase 4). Can be shipped as a web WASM app where users drag toggles and see wires light up.
- **Example Use Case:** `mimz repl` lets a student type `let out = a & !b`, then dynamically type `a = 1`, `b = 0` to see the live evaluation.

### 8.6 The Pipe Operator `|>` for Hardware Pipelines (Inspired by Elixir / F#)

- **Explanation:** Hardware design is fundamentally about data flowing through logic blocks. The pipe operator allows engineers to visually write combinational logic exactly how it looks on a schematic diagram, reading left-to-right instead of inside-out.
- **Example:**

  ```mimz
  // Instead of: truncate(apply_gain(filter_noise(raw_data), 2.0))
  let out = raw_data
      |> filter_noise()
      |> apply_gain(2.0)
      |> truncate()
  ```

### 8.7 The Spread Operator `..` for Wiring Modules (Inspired by JS / TypeScript)

- **Explanation:** In hardware, you frequently instantiate a module and pass signals that have the exact same names as your local wires (like clocks, resets, and buses). Verilog requires typing them all out redundantly (`.clk(clk), .rst(rst)`). The spread operator automates this.
- **Example:**

  ```mimz
  // Automatically wires up all matching names from 'standard_bus'
  let my_alu = ALU(..standard_bus, custom_flag: 1)
  ```

### 8.8 Struct/Bundle Update Syntax (Inspired by Rust)

- **Explanation:** When transitioning states in a Finite State Machine, you often want to keep 90% of a bundle/struct the same, but change one flag. The update syntax prevents massive boilerplate blocks.
- **Example:**

  ```mimz
  // Creates a new state bundle with the exact same values as 'old_state',
  // but overwrites the 'active' flag.
  let next_state = State { active: 1, ..old_state }
  ```

### 8.9 Chained Comparisons (Inspired by Python)

- **Explanation:** Checking if a hardware sensor value falls within a specific range requires verbose logical ANDs in C/Verilog (`if (val >= 0 && val <= 100)`). Python-style chained comparisons are much more mathematically readable.
- **Example:**

  ```mimz
  if (0 <= sensor_val <= 100) {
      // Safe operating range
  }
  ```

### 8.10 Modern Bit-Slicing & Concatenation (Inspired by Rust & ES6)

- **Explanation:** Verilog's syntax for grabbing bits `bus[7:0]` and concatenating them `{busA, busB}` is archaic, non-intuitive, and trips up beginners. Min-Mozhi can modernize this using standard range slicing and array spread syntax.
- **Example:**

  ```mimz
  // Slicing uses Rust-style ranges
  let upper_byte = data[8..16]

  // Concatenation uses spread arrays
  let combined_bus = [..upper_byte, ..lower_byte, padding_bit]
  ```

### 8.11 Vim-like TUI Workbench (`mimz tui`) — no-IDE interactive driver

- **Explanation:** a full-screen, keyboard-driven terminal UI (vim-like panes/modal
  keys) that wraps the whole toolchain so a user with **no IDE** still gets
  compiler help interactively. On start it asks the **output mode** — (a) emit
  Verilog, (b) just run the simulation and show the log (pass/fail + `$monitor`
  trace), or (c) also produce a waveform (VCD) — then opens an edit pane + a
  results pane that re-runs on save. Diagnostics (the friendly errors of 8.1) render
  inline against the source; `test` blocks run with their teaching messages; the
  waveform option writes a VCD and/or shows the console trace. Think "`mimz check`
  - `mimz sim`/`mimz test` + `mimz compile` behind one live TUI," not a new
    language surface.
- **How it differs from 8.5:** 8.5 is a narrow line REPL for **combinational**
  expressions (`let out = a & !b`, flip inputs, see the value). 8.11 is the
  **whole-design** workbench — clocked sim, waveforms, Verilog emit, test runs,
  inline diagnostics — for editing and running real `.mimz` files without an
  editor/IDE. 8.5's evaluator is one of the engines it drives.
- **Feasibility:** Medium, **tool not syntax** (zero language/freeze cost; additive,
  edition-safe). Rides what already ships: `src/sim` (Phase 1.5 kernel + VCD +
  `mimz test`), the emitter (`mimz compile`), the checker's diagnostics, and the
  trilingual front-end. A TUI crate (e.g. `ratatui`) would be the first real UI
  dependency — weigh against the minimal-dep ethos; the output-mode prompt + a
  re-run-on-save loop is the MVP. Post-Phase-1.5; pairs naturally with the Phase 4
  WASM playground (same engines, different shell).
- **Example Use Case:** `mimz tui counter.mimz` → prompt "Output: [v]erilog /
  [r]un+log / [w]aveform?" → pick `w` → edit pane on the left, on save the right
  pane shows the `test` results + a `$monitor` trace and writes `counter.vcd`; a
  width error underlines the offending wire with the E0301 teaching message.

### 8.12 Inline Test Modules & Auto-Generated Verilog Testbenches (`--emit-testbench`)

- **Status:** ✅ Implemented in Phase 4.
- **Explanation:** Modern languages (like Rust via `#[test]`) allow writing tests in the exact same file as the source code. Min-Mozhi already supports this via inline `test` blocks that run instantly in the built-in simulator. The next step is to add a CLI flag (`mimz compile --emit-testbench`) that automatically translates these inline `test` blocks into a standard, standalone Verilog `_tb.v` testbench. This allows engineers to rapidly write tests without leaving their `.mimz` file, while still outputting standard Verilog testbenches for external validation (like Icarus or EDA tools). Test blocks can be written inline or organized in a separate file.
- **Feasibility:** High. The compiler's differential test suite (`tests/icarus.rs`) already contains internal logic to generate Verilog testbenches from elaborated designs to cross-check against Icarus. Exposing this as a user-facing flag bridges the gap between fast inline iteration and industry-standard validation.
- **Example Use Case:** A user writes a module and its `test` block in `adder.mimz`. Running `mimz test` runs it natively. Running `mimz compile adder.mimz --emit-testbench` emits both `adder.v` and `adder_tb.v`, ready for external verification.

### 8.13 Project Scaffolding & Templates (`mimz init`)

- **Status:** 🟡 Base shipped (2026-06-25); template gallery is the open extension.
- **Explanation:** `cargo new`-style onboarding. The shipped `mimz init <name>` creates `./<name>/` with a documented `mimz.toml` and a starter `<name>.mimz` — a counter module (named PascalCase from the project) plus an inline `test` block that passes — so `mimz test` / `mimz compile` work on the first try with zero blank-page friction. The forward-looking extension is a **template gallery**: `mimz init <name> --template <kind>` choosing among curated starters (e.g. `counter`, `fsm`, `uart`, `alu`, `combinational`, `tamil` / `tanglish` flavor variants), and ideally sourcing them from the `examples/` corpus so every template is already part of the Icarus/golden differential — i.e. templates that are _proven to compile and simulate_, not hand-maintained snippets that can rot. A `--lang`/`--flavor` switch would emit the starter in Tanglish or pure-Tamil keywords, reinforcing the trilingual identity at first contact.
- **Feasibility:** High, **tool not syntax** (zero language/freeze cost; additive, edition-safe). The base landed in ~one file (`src/commands/init.rs`); the starter is modelled on the proven counter+test from `tests/test_run.rs`, and `tests/cli.rs` runs the generated project through `mimz test` so the scaffold can't silently break. The gallery's main design choice is the template source: inline string constants (simplest) vs. reading from `examples/` at build time (no rot, but couples `init` to the corpus layout). Flavor variants reuse `mimz translate`/`fmt`, which already reskin keywords — so a single English template can be emitted in any of the three flavors rather than maintaining three copies.
- **Example Use Case:** `mimz init blink --template fsm --flavor tanglish` → a `blink/` project whose `blink.mimz` is a documented traffic-light-style FSM written in Tanglish keywords, with a passing `test` block; `cd blink && mimz test` is green immediately.

---

### Tier 1 — Already shipped (the idea renames an existing rule)

| Idea                                     | Verdict                                                                                                                                                 |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2.8 Properties `{ get, private set }`    | Already enforced — an `out` is only drivable by its own module (E0104/E0107/E0108). Redundant; skip.                                                    |
| 2.1 Null safety (the goal)               | "A wire must always be driven" IS the single-driver + coverage pass (E0501/E0502). The `??` unwrap solves a problem the language already prevents.      |
| 1.1 Borrow checker (sound approximation) | Double-drives are already rejected. The full version (proving two FSM states never co-occur) is reachability analysis — SMT-grade; parked.              |
| 2.3 Width inference (half)               | `CtInt` + lossless `+` (max+1) is exactly the example. The other half — inferring a wire's declared type from its init — is a cheap, real win (Tier 3). |

### Tier 2 — Already planned (good design inputs for those slices)

| Idea                                                    | Lands with                                                                                                                                                             |
| ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1.4 Exhaustiveness                                      | Next checker slice. NEW spec question to rule on then: require a fallback arm even on fully-covered enums, because physical bit-flips can hit non-enum encodings.      |
| 1.2 Clock domain checking                               | = the deferred clock-ownership slice + Phase 2 multi-clock design. `@ clk` syntax and `sync.double_flop` are concrete inputs. Best idea in the doc relative to effort. |
| 3.3 await tests / 4.1 `sim::` / 6.4 step-back debugging | Phase 1.5 simulator API design. Step-back is feasible precisely because our own simulator records the full trace.                                                      |

### Tier 3 — Good and feasible — spec for Phase 2/3

| Idea                                                                    | Note                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 2.7 Tagged unions with payloads                                         | Strongest new feature; enums + match exist, payload = tag bits + max-payload bits, clean synthesis. Gives `Result` (4.2) for free. Build first after Phase 1.                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| 2.4 Interfaces/bundles + destructuring                                  | The #1 feature every Verilog successor adds; flatten to nets in the emitter. Then 2.9 structural matching is a small checker rule on top.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| 3.1 Channels — tier (a) only                                            | Decoupled-style valid/ready/data bundles with explicit handshake: proven tech. Tier (b) — blocking `<-` reads that auto-synthesize an FSM — is behavioral synthesis (research); parked.                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| 6.3 `prove` blocks                                                      | Do NOT embed Z3. Emit SystemVerilog assertions and drive SymbiYosys, the same way Icarus handles simulation. Weekend-sized backend, killer teaching feature.                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| 2.3 (other half) — **attempted 2026-06-14, DEFERRED**                   | `wire sum = a + b` without an annotation. NOT as cheap as it looked: the checker's `Ty` lives only inside the widths pass and has **no symbolic width algebra** (every module is checked under CONCRETE param bindings), so a wire in a parametric module (`bits[W+1]`) has no concrete type to write back into the AST; and the inferred type must be materialized for the emitter/sim, which the widths pass cannot do (it borrows the AST immutably). Needs symbolic widths (large) or a non-parametric-only restriction with a documented wart. Reason recorded in `docs/log/2026-06-14.md`; revisit when symbolic widths exist. |
| `count_ones`-style builtins — **partly addressed 2026-06-14**           | The reduction family already exists as the `&`/`\|`/`^` operators; `min`/`max`/`abs`/`nand`/`nor`/`xnor` shipped as built-ins (spec/02 v0.2.7). `count_ones` (popcount) itself is DEFERRED — it needs the operand's width at emit time, which the emitter cannot produce (`width()` works on a declared `Type` only). Revisit alongside emitter expression-width support.                                                                                                                                                                                                                                                            |
| **6.2 `secret` taint — slice 1** (promoted from Tier 4, v0.3)           | Now a G5 constitution goal. Lattice labels in a checker pass (SecVerilog model), `secret`/`declassify` keywords, error when secret reaches a public out or unlabelled storage. Explicit flow only — timing side channels stay out of scope, stated honestly.                                                                                                                                                                                                                                                                                                                                                                         |
| **4.3 `system_fault` silicon v1** (promoted from Tier 4, v0.3)          | Sticky fault reg + `FAULT_OUT` pin + safe-state mux on declared outputs + lockout until cold reset. Plain synthesizable logic, no clock gating (clock-stop stays parked). Sim side (`$fatal`) lands earlier, Phase 1.5.                                                                                                                                                                                                                                                                                                                                                                                                              |
| **2.1 `?` as valid-bundle** (re-targeted, v0.3)                         | `bits[8]?` = `{valid: bit, data: bits[8]}` (Chisel `Valid` style); `??` = mux on valid. Composes with channels tier-(a). Blocked on interfaces/bundles landing first. The tri-state meaning stays dead (Tier 4).                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `default` assignments (salvaged from 3.2 `defer`)                       | `default x = 0` = value unless assigned this cycle. Same forgot-to-deassert protection `defer` wanted, with honest hardware semantics. Small sugar.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `pipeline(stages = N)` honest version (salvaged from 6.1)               | Inserts N register stages + emits the vendor retiming attribute; the synthesis tool balances. Never promises "highest Fmax". Pairs with channels for latency bookkeeping.                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| Const-`if` at item level (salvaged from 2.6)                            | Conditional elaboration (Verilog `generate if`) — the real 10% params don't cover. Small feature, big DX for parameterized IP.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| **1.1 via `prove`** (promoted from Tier 4, second re-triage 2026-06-12) | Prove-backed shared-resource access: the user STATES the exclusion property (`prove !(fsm1 == CALC && fsm2 == CALC)`), SymbiYosys proves it, the checker then accepts the guarded double-drive. Full borrow-checker DX, sound, zero in-house SMT. Blocked on 6.3 landing.                                                                                                                                                                                                                                                                                                                                                            |

### Tier 4 — Rejected (with reasons that survive the v0.3 goal shift)

| Idea                                       | Reason                                                                                                                                                                                                                                                                              |
| ------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2.1 `bits[8]?` → tri-state                 | Internal tri-states are not synthesizable on FPGAs (IO pads only) — physics, not priorities. The `?` syntax survives as a valid-bundle (Tier 3). Top-level `inout` pads (I2C SDA, shared external buses) are a separate legitimate future feature — pad-level only, never internal. |
| 3.2 `defer` (the name and model)           | "Run on state transition" has no hardware meaning — detecting a transition is itself more hardware. The pain point survives as `default` assignments (Tier 3).                                                                                                                      |
| 6.1 Automatic retiming (the claim)         | Register balancing needs timing models only the synthesis tool has. The honest stage-inserter survives as `pipeline(stages = N)` (Tier 3).                                                                                                                                          |
| 2.6 Module-returning closures              | Parameterized modules already express the example (`DelayLine(CYCLES = 5)`); full metaprogramming is why Chisel embedded in Scala. The gap survives as const-`if` (Tier 3).                                                                                                         |
| 1.6 `pulse` / 1.3 affine tokens            | Research-grade temporal typing (see Filament); the 1.6 example violates E0505. Cheap approximation when channels land: unused-channel-read lint (must-consume warning).                                                                                                             |
| 2.5 `filter`                               | Hardware cannot have runtime-sized results — physics. `map`/`reduce` stay (sugar over `repeat`); `repeat` + const-`if` covers compile-time selection.                                                                                                                               |
| 1.1 Full temporal borrow check (automatic) | The COMPILER proving two FSM states never co-occur is reachability analysis (SMT-grade) — stays rejected. The sound approximation is today's single-driver rules (Tier 1); the user-stated, tool-proved version moved to Tier 3 ("1.1 via `prove`", second re-triage).              |

### Cross-cutting costs (price every Tier 3 item with these)

- Every new keyword (`chan`, `prove`, `interface`, `await`, …) needs Tanglish +
  Tamil spellings through lang/keywords.toml and native-speaker review — the keyword
  table is the bottleneck on every idea here.
- Every feature ships ×4 example folders (byte-identical Verilog rule), roughly
  doubling its apparent size.
- Several samples above contradict the current spec (wires assigned inside `on`,
  `=`/`<-` mixing) — each Tier 3 item needs a real spec section before code.

---

## 9. section 8 deep triage (2026-06-13): sugar vs breaking, under the freeze doctrine

### Growth doctrine (Decision 2026-06-13)

**Break freely until v0.1.0, then freeze; apply a breaking change only if it
benefits the language's future; use Editions + `mimz translate` as the migration
path** after the freeze. The repo is private and pre-v0.1.0 (no users), so a
breaking change is nearly free _now_ and expensive _later_.

**Organizing insight:** changes fall into two kinds.

- An _additive_ change (turns an error into valid code, or adds syntax that didn't
  exist) is edition-safe — it can land any time, even post-freeze, without breaking
  code.
- A _breaking_ change (re-means or removes existing valid syntax) must land
  **before v0.1.0** or owe an edition + `translate` rule.

So the freeze deadline pressures **only the section 8 ideas that touch
already-shipped syntax**: 8.9 and 8.10. The other eight are additive and can come
whenever.

### Per-idea verdicts

| Idea                               | Path                  | Tier               | Recommendation                                                                                                                                                                      |
| ---------------------------------- | --------------------- | ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 8.1 Elm-style didactic errors      | additive              | 2                  | Build incrementally now — it IS the G1 promise. Extend `Diag` + a `mimz explain <CODE>` long-form. Diagrams must depict real hardware (honesty).                                    |
| 8.2 Contracts `requires`/`ensures` | additive              | 3 (after `prove`)  | Edition-safe. Caller-side `requires` (compile-time div-by-zero) is the high-value half. Reserve the two keywords now.                                                               |
| 8.3 Fixed-point `fixed[N,F]`       | additive              | 3                  | Highest standalone educational/DSP value. Needs float literals + a rounding/overflow spec section (the honest part). Reserve `fixed` now.                                           |
| 8.4 `$comptime` / `$if`            | additive (split)      | 3 / 4              | Adopt item-level const-`if` (a **keyword**, not a `$` sigil). Reject the general comptime interpreter — `repeat`+const-`if` cover ~90%.                                             |
| 8.5 Hardware REPL                  | tool, not syntax      | 3 (Phase 4)        | Rides the approved WASM playground + Phase 1.5 sim evaluator. Scope to combinational. No syntax cost.                                                                               |
| 8.6 Pipe `\|>`                     | additive              | 3 (blocked)        | Needs callables (only builtins exist, E1110) AND is a 2nd way to write calls (G1 one-way). Park until extension functions land.                                                     |
| 8.7 Spread `..bus` (wiring)        | additive              | 3 (after bundles)  | Rank-1 honesty tension — implicit wiring hides connectivity. Allow only spreading a **declared interface type**; keep expansion greppable.                                          |
| 8.8 Struct update `..old`          | additive              | 3 (after bundles)  | Clean FSM ergonomics, low risk. Base is named, stays honest. `struct` already reserved.                                                                                             |
| 8.9 Chained comparison             | **additive widening** | ✅ DONE 2026-06-13 | **Allowed** — monotonic one-direction chain desugars to `&&` (`comparison_chain` in `src/parser/expr.rs`); mixed-direction + `==`/`!=` chains stay E1109. spec/02 v0.2.6 section 3. |
| 8.10 Range slice `[8..16]`         | **breaking**          | ✅ DONE 2026-06-13 | **Ratified `[hi:lo]` as final; break rejected** — universal hardware vocabulary wins; no range form. spec/02 v0.2.6 section 1.8.                                                    |

### The `..` operator (recommendation)

Use `..` for the **spread/splat family only** — wiring (8.7), struct-update (8.8),
concat-spread — because those are genuinely one operation (expand-a-bundle-in-place),
so one token is honest and learnable.

**Do NOT overload `..` for ranges** (keep slicing `[hi:lo]`, per 8.10) — that avoids
two problems:

- the range/splat semantic collision,
- and the ascending-exclusive vs descending-inclusive mental-model clash.

All `..`-spread features gate on interfaces/bundles (2.4); finalize the token when
2.4 is specced.

### Pre-v0.1.0 freeze checklist (what the doctrine forces now)

1. **Reserve keywords** (non-breaking, protects the namespace): `fixed`, `requires`,
   `ensures` — same pipeline as the eight v0.3-backlog words.
2. **Reserve the `..` spread operator** when interfaces/bundles (2.4) are specced
   (lexer/grammar matter, not the keyword table).
3. ~~**Decide 8.9**~~ ✅ **DONE 2026-06-13** — monotonic chained comparison allowed
   (`comparison_chain`, spec/02 v0.2.6 section 3).
4. ~~**Ratify `[hi:lo]` slicing as final**~~ ✅ **DONE 2026-06-13** — break rejected,
   `[hi:lo]`/`{a,b}` are canonical (spec/02 v0.2.6 section 1.8).
5. Everything else (8.1, 8.2, 8.3, 8.5, 8.6, 8.7, 8.8) is additive / edition-safe →
   can land after v0.1.0 with no breakage; none of it pressures the freeze date.
6. **Reserve `extern`** (external-Verilog / black-box-IP module — `architectural_ideas.md`
   idea 3, the architecture open question "External Verilog module wrapping construct").
   The _feature_ is additive and lands Phase 2+, but the _keyword_ must be reserved
   now so a v0.1 program can't claim it as an identifier (R11). Full pipeline, same
   as the other reserved words: `lang/keywords.toml` `reserved` + spec/03 reserved
   table & changelog + the TextMate invalid pattern + a lexer reserved-word test.
   English-only until the feature lands and native review supplies the spellings.

---

## 10. HDL parity gap analysis (2026-06-15)

Reviewed Min-Mozhi against the full feature sets of **VHDL, Verilog, and
SystemVerilog** ("variables/types → operators → control → loops → subprograms →
concurrency → OOP → verification").

**Decision:** scope = _curated subset + broaden RTL parity_ — stay synthesizable,
safe-by-default, educational. Concretely:

- pull the big **synthesizable** RTL gaps forward;
- do **not** chase SV verification/OOP now (but keep that door open, see below).

"Full parity" is the wrong target — half the SV list is verification/software,
which violates tie-breaker #1 (hardware honesty). The right target is **complete
synthesizable-RTL coverage** + the safety/trilingual differentiators.

### Status key

✅ have · 🟢 add-now (small, additive) · 🟡 pull-forward (synthesizable, was later)
· 🔵 already-planned Phase 2+ · ⛔ permanent-out (physics/honesty) · 🟣 deferred
verification layer (revisitable).

### What we already have (often safer than the originals)

`bit`/`bits[N]`/`signed[N]`/`enum`; lossless `+ - *` + wrapping `+% -% *%`;
`<< >>`, `& | ^ ~`, reductions `&x |x ^x`; comparison + monotonic chaining;
`{a,b}` concat, `x[i]`, `x[hi:lo]`; `if`-expr (mandatory else) + exhaustive
`match`; `repeat` generate; modules + params + instances + imports +
instance-arrays; `on rise(clk)` + `<-` + sync reset; built-in `test`/`tick`/
`expect`; 10 builtins.

### Gaps, triaged

> **Audited 2026-07-12 against actual code state** (this table had drifted
> badly — most 🟢/🟡 rows below were already shipped and undocumented as
> such). Verified by grepping for the concrete AST/checker/emit symbols, not
> by re-reading old commit messages.

| Feature (V/VHDL/SV)                                                                                                        | Status | Note                                                                                                                                                                                   |
| -------------------------------------------------------------------------------------------------------------------------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| replication `{N{x}}`                                                                                                       | ✅     | `ExprKind::Replicate`, parser+checker+emit, compile-time N                                                                                                                             |
| falling-edge `on fall(clk)`                                                                                                | ✅     | `Edge::Fall`, wired through parser/checker/emit/sim (`negedge` sensitivity)                                                                                                            |
| memories / arrays / RAM (`mem`)                                                                                            | ✅     | `ModuleItem::Mem { name, ty, depth, init }` — shipped 2026-06-17; full parser/checker(widths+names+drivers)/emit/sim/pretty/lint/analysis                                              |
| struct / record + interfaces (`struct`/`interface`)                                                                        | ✅     | shipped as `bundle` — checker/emit/sim complete; bundle-width-model (T6, 2026-07-11) added arg/return shape-checking                                                                   |
| combinational `function`                                                                                                   | ✅     | this **is** `fn` — AST doc comment: "user-defined combinational function... pure and combinational, no registers, no clocks"; Specs 1-2 shipped it                                     |
| async reset / reset polarity                                                                                               | ✅     | `ModuleItem::Reset { is_async, .. }`, wired to emit (`posedge rst` added to sensitivity list when async)                                                                               |
| packages / namespacing                                                                                                     | ✅     | `QualIdent` namespace-keying in checker (names/widths), shipped 2026-07-02/03 (Phase-2-packages-namespacing, 570 tests)                                                                |
| tagged-union payloads (2.7)                                                                                                | ✅     | `EnumVariant`/`PayloadField` in AST, wired through emit_verilog (translit/module/expr)                                                                                                 |
| `sync loop` — cycle-iterating FSM+counter loop                                                                             | ✅     | Spec 4 of `phase-2-suzhal-loop.local.md`, shipped 2026-07-06, 13 commits — lowers to existing Port/Reg/On/Drive, no new emit/sim shape needed                                          |
| don't-care `match` (casex/casez)                                                                                           | ✅     | `Pattern::IntMask { value, mask }` in `ast/expr.rs`, e.g. `0b1?? => ...`; `examples/*/priority.mimz` — shipped 2026-06-17 (corrected after a bad first grep — see re-audit note below) |
| `sync` CDC (1.2, `sync.double_flop(...)`) · `prove`/contracts (6.3/8.2) · `secret`/`system_fault` (G5) · fixed-point (8.3) | 🔵     | confirmed still open — reserved keywords only (`secret`/`prove`/`fixed`/`requires`/`ensures`), no AST/checker/emit support yet                                                         |
| `foreach`                                                                                                                  | ✅     | sugar over `repeat`/bare `loop`, shipped 2026-07-13 — range + array/`mem`-element source forms, module-item and `on`-block/`fn`-body statement level                                   |
| Enum-variant construction `Enum.Variant(a, b)`                                                                             | 🟡     | confirmed still open (`docs/plan/phase-2-ir-synthesis.md` line ~101) — needs `ExprKind::EnumConstruct`, follow-up to tagged unions                                                     |
| ternary `?:`                                                                                                               | ⛔     | `if {} else {}` expr is the one way (G1)                                                                                                                                               |
| division `/` / modulo `%` operators                                                                                        | ⛔     | no cheap operator form; future stdlib divider module                                                                                                                                   |
| internal tri-state; auto-retiming-with-Fmax                                                                                | ⛔     | physics / honesty (Tier 4, section 7)                                                                                                                                                  |

### Loops (explicit — three honest hardware shapes)

1. **Compile-time unroll** — `repeat i: lo..hi` — ✅ have (≈ `generate`,
   SV statically-bounded `for`).
2. **Controlled loop (`loop`/`suzhal`/`சுழல்` bare form, `sync loop` cycle
   form)** — ✅ **DONE (2026-07-06)**, both shapes: bare `loop` elaborates to
   N unrolled copies (area cost); `sync loop` lowers to a real counter +
   state machine spanning cycles (time cost). Four dependency-ordered specs
   in `docs/plan/phase-2-suzhal-loop.local.md`, all shipped (Spec 1: `return`
   - statement-based `fn` bodies; Spec 2: array-typed `fn` params; Spec 3:
     bounded elaborate-time `loop`; Spec 4: `sync loop`).
3. **`foreach`** — ✅ **DONE (2026-07-13)**; sugar over (1)/(2), now that
   array/`mem` types exist to iterate over.

A data-dependent unbounded `while` has no fixed silicon → accepted **only** in a
bounded or FSM-lowered form, never free-running.

### Verification / OOP / DV — 🟣 deferred, revisitable (NOT permanent-out)

The deferred DV features: SV `class`/OOP, `rand`/constraints,
covergroup/coverpoint/cross, immediate + concurrent (SVA) assertions, `fork/join`,
dynamic/associative arrays, queues, mailboxes.

**User intent (2026-06-15):** _"in future if needed we will include verification
logic too."_

These form a separate **verification layer** (not RTL): they ride the **simulator
track** (Phase 1.5+) and the **`prove`** track, fenced from synthesis exactly like
today's `test` blocks. Pursuing the heavier DV pieces later is a deliberate
**co-goal amendment to spec/01** when the simulator is mature — recorded as a
future option, not a rejection.

Substitutes to build first (cover most needs):

- `test`/`tick`/`expect` (have);
- `sim::fatal`/`sim::warn` (Phase 1.5);
- `prove` → SymbiYosys (Phase 2, SVA-style);
- `requires`/`ensures` (Phase 2+).

### Recommended pull-forward order (synthesizable RTL) — updated 2026-07-12

Everything in this order shipped, confirmed against `docs/plan/phase-2-ir-synthesis.md`
(the actually-maintained tracker for this backlog — it was accurate the whole
time; this file had just drifted). Original order preserved below with
strikethrough, so the sequencing rationale stays legible:

1. ~~Small additive batch: replication `{N{x}}`, `on fall`, don't-care
   `match`~~ ✅ all done, 2026-06-17.
2. ~~Memories/arrays (`mem`)~~ ✅ done. 3. ~~Structs + interfaces~~ ✅ done. 4. ~~Combinational `function`~~ ✅ done (is `fn`).
3. ~~Async reset / polarity~~ ✅ done (active-high; active-low still open). 6. ~~Controlled loop (`suzhal` + `sync loop`) + `foreach`~~ ✅ done.
4. Phase-2 line: ~~tagged unions~~ ✅ done. Enum-variant construction syntax
   (`Enum.Variant(a, b)`), `sync` CDC, `prove`/contracts, `secret`/
   `system_fault`, fixed-point — still open (reserved keywords / AST gaps
   only, no checker/emit support). 8. Verification layer — future,
   post-simulator, spec/01 amendment.

**Remaining open items, in order:** enum-variant construction
syntax → `sync` CDC synchronizers → `prove`/contracts → `secret`/
`system_fault` → fixed-point → verification layer.

### Newly-tracked items (were missing from this plan / phase-2)

Combinational **functions**, **replication `{N{x}}`**, **don't-care match
patterns**, **packages/namespacing**, explicit **async-reset**, **`foreach`**, a
clarified **controlled-loop (`suzhal`)** spec, and the **deferred verification
co-goal**. These were added to the phase-2 backlog ("Language features"); all
now shipped.

---

## 11. External-review triage (2026-06-26)

A reviewer proposed six improvements. Triaged against the compiler as it exists
today (522 tests; spec/02 v0.2.12). **Four of the six already ship or are already
planned** — recorded here so the review is captured and not re-opened. Only one is
genuinely new, and it is freeze-safe.

Status key: ✅ already shipped · 🔵 already triaged + planned · 🟡 new, feasible,
needs a Decision · ⛔ rejected.

| #   | Reviewer item                                       | Reality                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    | Verdict                                                              |
| --- | --------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| 1   | "Operator precedence is missing"                    | spec/02 **section 3** already defines a full Rust-style precedence table (`unary → * → + - → << >> → & → ^ → \| → comparison → && → \|\|`). So `a + b * c` parses as `a + (b * c)`. The EBNF line the reviewer saw (`binExpr = unary { binOp unary }`) carries the comment "precedence table, section 3" right beside it.                                                                                                                                                                                                  | ✅ shipped — non-issue                                               |
| 2   | `let` for local expression values                   | Named combinational intermediates **already exist** as `wire name: type = expr` (and `reg`). `let` is deliberately reserved for **hardware instances**, not variables (documented anti-footgun, spec/02 section 1.5). The real ergonomics gap is the **mandatory type annotation** — that is exactly item 5. An expression-level `let … in …` is a _second_ way to name a value `wire` already names → G1 tension, low marginal value.                                                                                     | 🔵 core shipped (covered by `wire` + item 5); expr-`let` parked (G1) |
| 3   | User-defined functions                              | `fn`/`function` are **reserved** (R11); calling a user function is **E1110**. "Combinational `function`" is an open item in `phase-2-ir-synthesis.md` (gap section 10) — pure/stateless, inlined at emit, also unblocks pipe `\|>` (8.6).                                                                                                                                                                                                                                                                                  | 🔵 already planned (phase-2)                                         |
| 4   | Enum `match` exhaustiveness                         | **Already enforced**: **E0601** (match not exhaustive) + **E0602** (unreachable arm), with fixtures, spec/02 section 1.3. The reviewer's exact example (error if `Stop` is omitted) is E0601 today. Note the spec wrinkle: a `_` fallback is _required even on fully-covered enums_ (radiation-bit-flip recovery).                                                                                                                                                                                                         | ✅ shipped — exactly the request                                     |
| 5   | Type inference (`wire x = a + b`)                   | = "wire type inference (2.3 other half)", already in phase-2. **Attempted 2026-06-14 and DEFERRED**: the checker's `Ty` has no symbolic width algebra (modules are checked under concrete param bindings), so a wire in a parametric module has no concrete type to write back, and the inferred type can't be materialized for emit/sim. Revisit when symbolic widths exist.                                                                                                                                              | 🔵 planned, blocked (documented)                                     |
| 6   | Port-declaration grouping `in { a, b, c: bits[8] }` | **Genuinely new** — not in any prior doc. Mechanically trivial: pure parser sugar that desugars to N separate port decls; additive, edition-safe, **zero freeze cost**. The tension is **G1** ("one obvious way to do each thing") — it is a second surface for a declaration that already has one, the same basis on which ternary `?:` and Rust range-slicing were rejected. Honest verdict: feasible and cheap, but it is a **philosophy decision, not a mechanical one** — needs a logged G1 ruling before code (R13). | 🟡 new, feasible, **needs a Decision**                               |

**Net:** items 1 and 4 need no action (shipped); items 2-core, 3, 5 are already
planned/shipped (pointers above, no duplication). The only forward action is
item 6 (port grouping) and the expr-`let` sliver of item 2 — both freeze-safe
additive sugar gated on a **G1 ruling**, recorded in the phase-2 plan's
"section 8 additive ideas" list as decision-pending (not committed work).

## 12. `fn` module-scope capture (2026-07-18, from CTO review BUG-12)

Today a `fn` body only sees file-level consts/params, never the enclosing
module's — a module-const reference from inside a `fn` fails `mimz check`
with E0101, and the emitter agrees (`file_env` swap in
`emit_verilog/module.rs`). Filed as [`docs/audit/bugs.md`](../audit/bugs.md)
BUG-12, re-scoped 2026-07-17 from an emitter bug to a language-design
limitation (checker and emitter are consistent, not divergent).

**Status: open, deliberately deferred — not a bug to close, a feature to
design.** Two directions, neither picked yet:

- Bless file-scoping explicitly in `spec/02-syntax-and-grammar.md` (document
  the limitation as intentional; workaround stays "pass the value as a `fn`
  parameter, or hoist the const to file level").
- Design real module-scope capture for `fn` — needs its own spec section
  covering how a `fn`'s width/const resolution interacts with the module's
  own parametric instantiation; a checker + emitter change, not a doc-only
  fix.

Revisit after the `phase-2-correctness-consolidation` stages land (the
`fn`-scoping decision doesn't block those, and per that roadmap's own
recommendation, new language surface waits until the correctness class it
depends on — one shared width/const-eval authority — is closed).
