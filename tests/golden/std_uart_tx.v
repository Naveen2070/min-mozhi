module UartTx #(
    parameter CLKS_PER_BIT = 4
) (
    input wire clk,
    input wire rst,
    input wire start,
    input wire [(8)-1:0] data,
    output wire tx,
    output wire busy
);
    localparam [1:0] STATE_IDLE = 0;
    localparam [1:0] STATE_START = 1;
    localparam [1:0] STATE_DATA = 2;
    localparam [1:0] STATE_STOP = 3;
    reg [1:0] state;
    reg [(16)-1:0] clk_count;
    reg [(3)-1:0] bit_index;
    reg [(8)-1:0] shift;
    // NOTE (BUG-65, docs/audit/bugs.md): the `initial` register-init line(s) below are simulation/FPGA-only - an ASIC flow has no defined power-on default and will not honor them. The synchronous reset below still applies regardless.
    initial state = STATE_IDLE;
    initial clk_count = 0;
    initial bit_index = 0;
    initial shift = 0;
    assign tx = (((state == STATE_IDLE)) ? (1) : (((state == STATE_START)) ? (0) : (((state == STATE_DATA)) ? (shift[0]) : (1))));
    assign busy = (((state == STATE_IDLE)) ? (0) : (((state == STATE_START)) ? (1) : (((state == STATE_DATA)) ? (1) : (1))));
    always @(posedge clk) begin
        if (rst) begin
            clk_count <= 0;
            bit_index <= 0;
            shift <= 0;
            state <= STATE_IDLE;
        end else begin
            if ((state == STATE_IDLE)) begin
                clk_count <= 0;
                bit_index <= 0;
                if (start) begin
                    shift <= data;
                    state <= STATE_START;
                end
            end else begin
                if ((state == STATE_START)) begin
                    if ((clk_count == (CLKS_PER_BIT - 1))) begin
                        clk_count <= 0;
                        state <= STATE_DATA;
                    end else begin
                        clk_count <= (clk_count + 16'd1);
                    end
                end else begin
                    if ((state == STATE_DATA)) begin
                        if ((clk_count == (CLKS_PER_BIT - 1))) begin
                            clk_count <= 0;
                            shift <= (shift >> 1);
                            if ((bit_index == 7)) begin
                                bit_index <= 0;
                                state <= STATE_STOP;
                            end else begin
                                bit_index <= (bit_index + 3'd1);
                            end
                        end else begin
                            clk_count <= (clk_count + 16'd1);
                        end
                    end else begin
                        if ((clk_count == (CLKS_PER_BIT - 1))) begin
                            clk_count <= 0;
                            state <= STATE_IDLE;
                        end else begin
                            clk_count <= (clk_count + 16'd1);
                        end
                    end
                end
            end
        end
    end
endmodule

