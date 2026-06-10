# Phase 3 — Native Backend

> **Full end-to-end ownership.**
> Window: years 2–4 · Status: ⚪ not started

## Goal

`mimz build blink.mimz --target ice40` programs an FPGA **with no foreign
tool in the loop** — our own techmapping, place-and-route, and bitstream
generation for the iCE40 family (fully documented by Project IceStorm).

## Work items

- [ ] Techmapper: IR → iCE40 primitives (LUT4, DFF, carry chains, BRAM)
- [ ] Placer (start simple: simulated annealing)
- [ ] Router (pathfinder-style negotiated congestion)
- [ ] Bitstream generator from IceStorm chip databases
- [ ] Timing analysis (basic static timing, report worst paths)
- [ ] Optimizer passes matured: retiming candidates, logic sharing
- [ ] Differential validation: native flow vs Yosys/nextpnr flow on the full example suite
- [ ] Board support: iCEBreaker first; document adding a new board

## Milestone

LED blinker + UART echo built and flashed end-to-end with only `mimz`.

## Exit criteria

1. Native flow passes the differential suite vs the open toolchain.
2. At least two real designs (blinky, UART) run on hardware from the native flow.
3. Architecture doc covers the full backend.

## Risks / notes

- This is the longest, hardest phase — keep the Yosys/nextpnr path working the
  whole time as the reference and fallback.
- Scope to **iCE40 only** until exit criteria pass; ECP5 is the logged
  candidate for target #2.
