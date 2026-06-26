module Fifo #(
    parameter WIDTH = 8,
    parameter DEPTH = 4
) (
    input wire clk,
    input wire rst,
    input wire push,
    input wire pop,
    input wire [(WIDTH)-1:0] din,
    output wire full,
    output wire empty,
    output wire [(WIDTH)-1:0] dout
);
    function integer clog2;
        input integer value;
        integer i;
        begin
            if (value <= 1) clog2 = 1;
            else begin
                clog2 = 0;
                for (i = value - 1; i > 0; i = i >> 1) clog2 = clog2 + 1;
            end
        end
    endfunction
    reg [(WIDTH)-1:0] data [0:(DEPTH)-1];
    reg [(clog2(DEPTH))-1:0] head;
    reg [(clog2(DEPTH))-1:0] tail;
    reg [((clog2(DEPTH) + 1))-1:0] count;
    integer __mimz_data_i;
    initial for (__mimz_data_i = 0; __mimz_data_i < (DEPTH); __mimz_data_i = __mimz_data_i + 1) data[__mimz_data_i] = 0;
    assign dout = data[head];
    assign full = (count == DEPTH);
    assign empty = (count == 0);
    always @(posedge clk) begin
        if (rst) begin
            tail <= 0;
            head <= 0;
            count <= 0;
        end else begin
            if ((push && (count != DEPTH))) begin
                data[tail] <= din;
                if ((tail == (DEPTH - 1))) begin
                    tail <= 0;
                end else begin
                    tail <= (tail + 1);
                end
                if ((pop && (count != 0))) begin
                    if ((head == (DEPTH - 1))) begin
                        head <= 0;
                    end else begin
                        head <= (head + 1);
                    end
                end else begin
                    count <= (count + 1);
                end
            end else begin
                if ((pop && (count != 0))) begin
                    if ((head == (DEPTH - 1))) begin
                        head <= 0;
                    end else begin
                        head <= (head + 1);
                    end
                    count <= (count - 1);
                end
            end
        end
    end
endmodule

