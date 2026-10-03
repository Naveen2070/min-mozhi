module ExternDemo (
    input wire sysclk,
    output wire fast_clk,
    output wire pll_ok
);
    wire u_clk_out;
    wire u_locked;
    Pll #(.MULT(4)) u0 (.clk_in(sysclk), .clk_out(u_clk_out), .locked(u_locked));
    assign fast_clk = u_clk_out;
    assign pll_ok = u_locked;
endmodule
