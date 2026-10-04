//! Built-in board presets for `mimz build --board`.

/// A board: the FPGA it carries and the pin of each preset port name.
pub struct Board {
    pub name: &'static str,
    /// nextpnr-ice40 device flag without dashes (`up5k` -> `--up5k`).
    pub device: &'static str,
    pub package: &'static str,
    /// The board's oscillator, the default `--freq` target.
    pub freq_mhz: u32,
    /// `(preset port name, FPGA pin)`.
    pub pins: &'static [(&'static str, &'static str)],
}

/// 1BitSquared iCEBreaker v1.0. Pins from the official `icebreaker.pcf`
/// (`icebreaker-fpga/icebreaker-examples` at commit `3b0a6c9`, the last
/// revision before the file left that repository's main branch; checked
/// 2026-10-04). LEDs and the user button are active-low (`_n`).
pub const ICEBREAKER: Board = Board {
    name: "icebreaker",
    device: "up5k",
    package: "sg48",
    freq_mhz: 12,
    pins: &[
        ("clk", "35"),
        ("tx", "9"),
        ("rx", "6"),
        ("btn_n", "10"),
        ("led_r_n", "11"),
        ("led_g_n", "37"),
        ("btn1", "20"),
        ("btn2", "19"),
        ("btn3", "18"),
        ("led1", "26"),
        ("led2", "27"),
        ("led3", "25"),
        ("led4", "23"),
        ("led5", "21"),
    ],
};

const BOARDS: &[&Board] = &[&ICEBREAKER];

/// The preset named `name`, if there is one.
pub fn board(name: &str) -> Option<&'static Board> {
    BOARDS.iter().copied().find(|b| b.name == name)
}

/// Every preset's name, for `--board` help and errors.
pub fn names() -> Vec<&'static str> {
    BOARDS.iter().map(|b| b.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icebreaker_is_up5k_sg48_at_12_mhz() {
        let b = board("icebreaker").expect("icebreaker preset");
        assert_eq!((b.device, b.package, b.freq_mhz), ("up5k", "sg48", 12));
        assert!(b.pins.contains(&("clk", "35")));
        assert_eq!(names(), vec!["icebreaker"]);
    }

    #[test]
    fn an_unknown_board_is_none() {
        assert!(board("nexys").is_none());
    }
}
