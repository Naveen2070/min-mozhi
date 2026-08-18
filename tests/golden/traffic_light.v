module TrafficLight (
    input wire clk,
    input wire rst,
    output wire red,
    output wire yellow,
    output wire green
);
    localparam [1:0] STATE_RED = 0;
    localparam [1:0] STATE_GREEN = 1;
    localparam [1:0] STATE_YELLOW = 2;
    reg [1:0] state;
    reg [(8)-1:0] timer;
    // NOTE (BUG-65, docs/audit/bugs.md): the `initial` register-init line(s) below are simulation/FPGA-only - an ASIC flow has no defined power-on default and will not honor them. The synchronous reset below still applies regardless.
    initial #0 state = STATE_RED;
    initial #0 timer = 0;
    assign red = (state == STATE_RED);
    assign yellow = (state == STATE_YELLOW);
    assign green = (state == STATE_GREEN);
    always @(posedge clk) begin
        if (rst) begin
            state <= STATE_RED;
            timer <= 0;
        end else begin
            if ((timer == 0)) begin
                state <= (((state == STATE_RED)) ? (STATE_GREEN) : (((state == STATE_GREEN)) ? (STATE_YELLOW) : (STATE_RED)));
                timer <= (((state == STATE_RED)) ? (50) : (((state == STATE_GREEN)) ? (40) : (10)));
            end else begin
                timer <= (timer - 8'd1);
            end
        end
    end
endmodule

