# Hardware Emulation Mapping (`sim` blocks)

## The Vision

Min-Mozhi’s built-in simulator (`mimz sim` and `mimz test`) is cycle-accurate and bit-for-bit identical to real hardware. To make hardware design more interactive and rewarding, we are introducing **Physical Hardware Emulation**.

Instead of relying on "magic" port names, the language will introduce an explicit `sim { ... }` block. This gives the hardware designer explicit control over how simulation output behaves physically on their computer, while keeping the core logic pure.

## Core Principles

1. **Explicit Mapping in Tests:** Simulation bindings are explicitly declared inside `test` blocks. Modules remain pure and agnostic to how they are tested or simulated.
2. **Zero Verilog Impact:** The `mimz compile` command will completely ignore `sim` blocks. They produce no Verilog output.
3. **Secure & Vetted Dependencies:** The compiler backend will only use highly vetted, maintained Rust crates (e.g., `cpal`, `crossterm`) to ensure security and cross-platform stability.
4. **WASM Fallback:** In Phase 1, physical emulation bindings are parsed but act as a no-op in the `mimz-wasm` web environment, focusing strictly on native CLI support first.

---

## Proposed Syntax

A `sim` block is placed inside a `test` block. It defines the real-world clock speed and bindings between module ports and virtual peripherals.

### English Flavor

```javascript
test "play music" for MelodyPlayer {
  // Setup inputs
  start = 1

  // Explicit mapping for physical emulation
  sim {
    // Defines real-world clock speed for real-time throttling
    speed mhz(50)

    // Bind ports to physical peripherals
    bind audio -> speaker(waveform: "square")
    bind playing -> led(color: "green")
  }

  // Run simulation
  tick(clk, 50000000) // run for 1 real-world second
}
```

### Tamil / Tanglish Translations (Pending Native Review)

- `sim` -> `paavnai` / `பாவனை` (simulation/emulation)
- `bind` -> `inai` / `இணை` (connect/bind)
- `speed` -> `vegam` / `வேகம்` (speed)
- `speaker` -> `oli` / `ஒலி` (sound/audio)
- `led` -> `vilakku` / `விளக்கு` (light)

---

## Supported Virtual Peripherals (Phase 1)

### 1. `speaker` (Audio Emulation)

- **Target:** Single `bit` output.
- **Behavior:** Streams the raw bit toggles (e.g., PWM or square waves) directly to the host machine's audio output.
- **Rust Backend Crate:** `cpal` (Maintained by the RustAudio project; cross-platform, secure).

### 2. `led` (Visual Emulation)

- **Target:** Single `bit` or `bits[N]`.
- **Behavior:** Renders a colored indicator in the terminal UI when the simulation runs. If the bit is `1`, the LED is on.
- **Rust Backend Crate:** `crossterm` or `ratatui` (Secure, lightweight terminal manipulation).

### 3. `uart_rx` / `uart_tx` (Serial/Network Emulation)

- **Target:** Single `bit`.
- **Behavior:** The simulator attaches a virtual UART driver to the pin. It decodes the 8-N-1 serial protocol on the fly and prints the decoded ASCII text to the terminal (or pipes it to a local socket).
- **Rust Backend Crate:** Standard library (`std::io` / `std::net`). No third-party crates required.

---

## Execution Model

1. **Parsing:** The parser adds a `SimBlock` node to the AST inside `TestBlock`. The `speed mhz(50)` uses standard function-call syntax to avoid complicating the numeric lexer.
2. **Checking:** The type-checker ensures that mapped ports actually exist on the instanced module and have valid widths. It also validates `hz`, `khz`, and `mhz` built-ins.
3. **Simulation (`src/sim/harness.rs`):**
   - If a test contains a `sim` block, the simulation loop throttles its execution speed to match the real-world time elapsed according to the `speed` parameter.
   - For each `bind`, it initializes a background thread (e.g., an audio stream).
   - Inside the `tick(clk)` loop, the new state of the bound pin is immediately pushed to a fast, lock-free channel communicating with the peripheral thread.
