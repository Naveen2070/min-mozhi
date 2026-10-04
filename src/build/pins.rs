//! Port -> FPGA pin mapping for `mimz build`: a board preset, overridden by
//! a user PCF, checked against the top module's ports.

use super::boards::Board;
use std::collections::BTreeMap;

#[derive(Debug)]
pub enum PinError {
    /// A PCF line that is not `set_io [flags] NAME PIN` (1-based line).
    BadLine { line: usize, text: String },
    /// Top-level port bits with no pin (`led`, `leds[3]`).
    Unpinned(Vec<String>),
    /// User PCF names that are not top-level ports.
    NotAPort(Vec<String>),
}

/// `(name, pin)` of every `set_io` line; `#` comments, blank lines and
/// `-flag` words are skipped.
pub fn parse_pcf(text: &str) -> Result<Vec<(String, String)>, PinError> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let words: Vec<&str> = line
            .split_whitespace()
            .filter(|w| !w.starts_with('-'))
            .collect();
        match words.as_slice() {
            ["set_io", name, pin] => out.push((name.to_string(), pin.to_string())),
            _ => {
                return Err(PinError::BadLine {
                    line: i + 1,
                    text: raw.to_string(),
                });
            }
        }
    }
    Ok(out)
}

/// `name` for a 1-bit port, `name[i]` for each bit of a vector.
fn bit_names(name: &str, width: u32) -> Vec<String> {
    if width == 1 {
        vec![name.to_string()]
    } else {
        (0..width).map(|i| format!("{name}[{i}]")).collect()
    }
}

/// Pins every bit of every top-level port. `ports` is `(IR name, Verilog
/// name, width)`; a PCF may name a port by either. The board preset pins
/// the ports whose names it knows, the user PCF overrides it. Returns
/// `(Verilog bit name such as led or leds[2], FPGA pin)` in port order.
pub fn resolve(
    board: Option<&Board>,
    user: &[(String, String)],
    ports: &[(String, String, u32)],
) -> Result<Vec<(String, String)>, PinError> {
    // Verilog bit name for every name a PCF may use (source or Verilog).
    let mut alias: BTreeMap<String, String> = BTreeMap::new();
    let mut required: Vec<String> = Vec::new();
    for (ir, verilog, w) in ports {
        for (s, v) in bit_names(ir, *w).into_iter().zip(bit_names(verilog, *w)) {
            alias.insert(s, v.clone());
            alias.insert(v.clone(), v.clone());
            required.push(v);
        }
    }
    let not_ports: Vec<String> = user
        .iter()
        .filter(|(n, _)| !alias.contains_key(n))
        .map(|(n, _)| n.clone())
        .collect();
    if !not_ports.is_empty() {
        return Err(PinError::NotAPort(not_ports));
    }
    let mut pins: BTreeMap<String, String> = BTreeMap::new();
    if let Some(b) = board {
        for (n, p) in b.pins {
            if let Some(v) = alias.get(*n) {
                pins.insert(v.clone(), p.to_string());
            }
        }
    }
    for (n, p) in user {
        pins.insert(alias[n].clone(), p.clone());
    }
    let missing: Vec<String> = required
        .iter()
        .filter(|v| !pins.contains_key(*v))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(PinError::Unpinned(missing));
    }
    Ok(required
        .into_iter()
        .map(|v| {
            let p = pins[&v].clone();
            (v, p)
        })
        .collect())
}

/// A nextpnr PCF: one `set_io NAME PIN` line per pin.
pub fn write_pcf(pins: &[(String, String)]) -> String {
    pins.iter()
        .map(|(n, p)| format!("set_io {n} {p}\n"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::boards::board;

    fn ports(list: &[(&str, &str, u32)]) -> Vec<(String, String, u32)> {
        list.iter()
            .map(|(a, b, w)| (a.to_string(), b.to_string(), *w))
            .collect()
    }

    #[test]
    fn parse_pcf_reads_set_io_lines_and_skips_comments() {
        let p = parse_pcf("# pins\nset_io led 11\nset_io -nowarn btn_n 10\n\n").unwrap();
        assert_eq!(
            p,
            vec![("led".into(), "11".into()), ("btn_n".into(), "10".into())]
        );
    }

    #[test]
    fn a_malformed_pcf_line_is_an_error() {
        assert!(matches!(
            parse_pcf("set_io led\n"),
            Err(PinError::BadLine { line: 1, .. })
        ));
    }

    #[test]
    fn the_board_preset_pins_a_port_with_a_preset_name() {
        let got = resolve(board("icebreaker"), &[], &ports(&[("clk", "clk", 1)])).unwrap();
        assert_eq!(got, vec![("clk".into(), "35".into())]);
    }

    #[test]
    fn a_user_pcf_overrides_the_preset() {
        let user = vec![("clk".into(), "20".into())];
        let got = resolve(board("icebreaker"), &user, &ports(&[("clk", "clk", 1)])).unwrap();
        assert_eq!(got, vec![("clk".into(), "20".into())]);
    }

    #[test]
    fn a_port_with_no_pin_is_reported_by_name() {
        let err = resolve(
            board("icebreaker"),
            &[],
            &ports(&[("clk", "clk", 1), ("led", "led", 1)]),
        )
        .unwrap_err();
        assert!(matches!(err, PinError::Unpinned(v) if v == vec!["led".to_string()]));
    }

    #[test]
    fn a_pcf_name_that_is_not_a_port_is_an_error() {
        let user = vec![("ledd".into(), "11".into())];
        let err = resolve(None, &user, &ports(&[])).unwrap_err();
        assert!(matches!(err, PinError::NotAPort(v) if v == vec!["ledd".to_string()]));
    }

    #[test]
    fn every_bit_of_a_vector_port_needs_a_pin() {
        let user: Vec<(String, String)> = (0..4)
            .map(|i| (format!("leds[{i}]"), format!("{}", 20 + i)))
            .collect();
        let err = resolve(None, &user, &ports(&[("leds", "leds", 5)])).unwrap_err();
        assert!(matches!(err, PinError::Unpinned(v) if v == vec!["leds[4]".to_string()]));
    }

    #[test]
    fn a_pcf_can_name_a_port_by_its_source_or_verilog_name() {
        // Tamil source name `விளக்கு` emitted as an ASCII Verilog name.
        let p = ports(&[("விளக்கு", "vilakku", 1)]);
        for name in ["விளக்கு", "vilakku"] {
            let got = resolve(None, &[(name.into(), "11".into())], &p).unwrap();
            assert_eq!(got, vec![("vilakku".into(), "11".into())]);
        }
    }

    #[test]
    fn write_pcf_writes_one_set_io_per_pin() {
        assert_eq!(write_pcf(&[("led".into(), "11".into())]), "set_io led 11\n");
    }
}
