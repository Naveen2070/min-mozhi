`timescale 1ns/1ps
module sc_uart_echo_tb;
    reg clk = 0;
    reg rst = 1;
    reg rx;
    wire tx, busy;
    wire [7:0] received;
    UartEcho #(.CLKS_PER_BIT(4)) dut (.clk(clk), .rst(rst), .rx(rx), .tx(tx), .busy(busy), .received(received));
    always #5 clk = ~clk;
    task tick; begin @(posedge clk); #1; end endtask
    task uart_send(input [7:0] byte_val);
        integer i;
        begin
            // Start bit (hold 0 for CLKS_PER_BIT+1 = 5 cycles)
            rx = 0; repeat (5) tick();
            // Data bits LSB first (hold each for CLKS_PER_BIT = 4 cycles)
            for (i = 0; i < 8; i = i + 1) begin
                rx = byte_val[i]; repeat (4) tick();
            end
            // Stop bit
            rx = 1; repeat (4) tick();
        end
    endtask
    initial begin
        rx = 1;
        tick(); tick();                         // idle for 2 cycles
        rst = 0;                                // de-assert reset

        uart_send(8'hA5);                       // send 0xA5
        repeat (10) tick();                     // wait for rx_byte capture

        if (received !== 8'hA5) begin
            $display("FAIL: expected 0xA5, got %h", received);
            $finish;
        end

        uart_send(8'h33);                       // send 0x33
        repeat (10) tick();

        if (received !== 8'h33) begin
            $display("FAIL: expected 0x33, got %h", received);
            $finish;
        end

        $display("PASS");
        $finish;
    end
endmodule
