// Self-checking TB: theeti — the pure-Tamil twin of `fn_array_search_tb.v`
// (english/fn_array_search.mimz -> tamil-pure/fn_array_search.mimz), proving
// the array-typed fn parameter lowering and the runtime-index mux simulate
// correctly through the ROMANIZED Tamil identifiers, not just that they
// elaborate. Same values, same cases (including both duplicate-match
// scenarios) as the English original — only names differ:
// a/b/c/d/target/pick_idx/idx/picked -> a/aa/i/ii/ilakku/sutti/itam/vitai.
`timescale 1ns/1ps
module theeti_tb;
  reg [7:0] a, aa, i, ii, ilakku;
  reg [2:0] sutti;
  wire signed [3:0] itam;
  wire [7:0] vitai;
  theeti dut (
      .a(a),
      .aa(aa),
      .i(i),
      .ii(ii),
      .ilakku(ilakku),
      .sutti(sutti),
      .itam(itam),
      .vitai(vitai)
  );

  task check(input [7:0] xa, xaa, xi, xii, xilakku, input [2:0] xsutti,
             input signed [3:0] xitam, input [7:0] xvitai);
    begin
      a = xa; aa = xaa; i = xi; ii = xii; ilakku = xilakku; sutti = xsutti; #1;
      if (itam !== xitam) begin
        $display("FAIL: theetu(%0d,%0d,%0d,%0d,%0d) -> %0d, expected %0d",
                  xa, xaa, xi, xii, xilakku, itam, xitam);
        $finish;
      end
      if (vitai !== xvitai) begin
        $display("FAIL: theervu(%0d,%0d,%0d,%0d, sutti=%0d) -> %0d, expected %0d",
                  xa, xaa, xi, xii, xsutti, vitai, xvitai);
        $finish;
      end
    end
  endtask

  initial begin
    // theetu (find_index) coverage (sutti held at 0 -> vitai == a, checked too).
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd10, 3'd0, 0, 8'd10);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd20, 3'd0, 1, 8'd10);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd30, 3'd0, 2, 8'd10);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd40, 3'd0, 3, 8'd10);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd99, 3'd0, -1, 8'd10);

    // theervu runtime-index mux: in-range sweep (ilakku held fixed at 99 so
    // itam stays -1 throughout, isolating the vitai assertion).
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd99, 3'd0, -1, 8'd10);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd99, 3'd1, -1, 8'd20);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd99, 3'd2, -1, 8'd30);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd99, 3'd3, -1, 8'd40);

    // theervu out-of-range fallback: any sutti >= 4 must read back the last
    // element (ii) per the generated mux's own default-chain shape.
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd99, 3'd4, -1, 8'd40);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd99, 3'd5, -1, 8'd40);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd99, 3'd7, -1, 8'd40);

    // DUPLICATE MATCH: ilakku present at both index 0 (a) and index 2 (i).
    // The LOWER index (0) must still win.
    check(8'd77, 8'd20, 8'd77, 8'd40, 8'd77, 3'd0, 0, 8'd77);
    // Duplicate match at index 1 and 3: lower (1) must win.
    check(8'd10, 8'd55, 8'd30, 8'd55, 8'd55, 3'd0, 1, 8'd10);

    $display("PASS");
    $finish;
  end
endmodule
