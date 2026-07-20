module UartEcho #(
    parameter CLKS_PER_BIT = 4
) (
    input wire clk,
    input wire rst,
    input wire rx,
    output wire tx,
    output wire busy,
    output wire [(8)-1:0] received
);
    wire [7:0] __mimz_sub_1;
    assign __mimz_sub_1 = (shift >> 1);
    wire [7:0] __mimz_sub_2;
    assign __mimz_sub_2 = ((rx) << 7);
    localparam [1:0] RXSTATE_IDLE = 0;
    localparam [1:0] RXSTATE_START = 1;
    localparam [1:0] RXSTATE_DATA = 2;
    localparam [1:0] RXSTATE_STOP = 3;
    reg [1:0] rx_state;
    reg [(16)-1:0] baud_cnt;
    reg [(3)-1:0] bit_idx;
    reg [(8)-1:0] shift;
    reg [(8)-1:0] rx_byte;
    reg [(8)-1:0] echo_byte;
    reg echo_pending;
    reg tx_start;
    wire tx_inst_tx;
    wire tx_inst_busy;
    UartTx #(.CLKS_PER_BIT(CLKS_PER_BIT)) tx_inst (.clk(clk), .rst(rst), .start(tx_start), .data(echo_byte), .tx(tx_inst_tx), .busy(tx_inst_busy));
    assign tx = tx_inst_tx;
    assign busy = (((rx_state == RXSTATE_IDLE)) ? (tx_inst_busy) : (1));
    assign received = rx_byte;
    always @(posedge clk) begin
        if (rst) begin
            tx_start <= 0;
            baud_cnt <= 0;
            bit_idx <= 0;
            rx_state <= RXSTATE_IDLE;
            shift <= 0;
            rx_byte <= 0;
            echo_byte <= 0;
            echo_pending <= 0;
        end else begin
            tx_start <= 0;
            if ((rx_state == RXSTATE_IDLE)) begin
                baud_cnt <= 0;
                bit_idx <= 0;
                if ((rx == 0)) begin
                    rx_state <= RXSTATE_START;
                end else begin
                    rx_state <= RXSTATE_IDLE;
                end
            end else begin
                if ((rx_state == RXSTATE_START)) begin
                    bit_idx <= 0;
                    baud_cnt <= (baud_cnt + 1);
                    if ((baud_cnt == (CLKS_PER_BIT - 1))) begin
                        baud_cnt <= 0;
                        if ((rx == 0)) begin
                            rx_state <= RXSTATE_DATA;
                        end else begin
                            rx_state <= RXSTATE_IDLE;
                        end
                    end else begin
                        rx_state <= RXSTATE_START;
                    end
                end else begin
                    if ((rx_state == RXSTATE_DATA)) begin
                        baud_cnt <= (baud_cnt + 1);
                        if ((baud_cnt == (CLKS_PER_BIT - 1))) begin
                            baud_cnt <= 0;
                            shift <= (__mimz_sub_1 | __mimz_sub_2);
                            bit_idx <= (bit_idx + 1);
                            if ((bit_idx == 7)) begin
                                rx_state <= RXSTATE_STOP;
                            end else begin
                                rx_state <= RXSTATE_DATA;
                            end
                        end else begin
                            rx_state <= RXSTATE_DATA;
                        end
                    end else begin
                        bit_idx <= 0;
                        baud_cnt <= (baud_cnt + 1);
                        if ((baud_cnt == (CLKS_PER_BIT - 1))) begin
                            baud_cnt <= 0;
                            if ((rx == 1)) begin
                                rx_byte <= shift;
                                echo_byte <= shift;
                                echo_pending <= 1;
                            end
                            rx_state <= RXSTATE_IDLE;
                        end else begin
                            rx_state <= RXSTATE_STOP;
                        end
                    end
                end
            end
            if (echo_pending) begin
                tx_start <= 1;
                echo_pending <= 0;
            end
        end
    end
endmodule

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
                        clk_count <= (clk_count + 1);
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
                                bit_index <= (bit_index + 1);
                            end
                        end else begin
                            clk_count <= (clk_count + 1);
                        end
                    end else begin
                        if ((clk_count == (CLKS_PER_BIT - 1))) begin
                            clk_count <= 0;
                            state <= STATE_IDLE;
                        end else begin
                            clk_count <= (clk_count + 1);
                        end
                    end
                end
            end
        end
    end
endmodule

