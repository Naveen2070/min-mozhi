module Fifo #(
    parameter WIDTH = 8,
    parameter AW = 2
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
    reg [(WIDTH)-1:0] data [0:((1 << AW))-1];
    reg [(AW)-1:0] head;
    reg [(AW)-1:0] tail;
    reg [((AW + 1))-1:0] count;
    integer __mimz_data_i;
    initial for (__mimz_data_i = 0; __mimz_data_i < ((1 << AW)); __mimz_data_i = __mimz_data_i + 1) data[__mimz_data_i] = 0;
    assign dout = data[head];
    assign full = (count == (1 << AW));
    assign empty = (count == 0);
    always @(posedge clk) begin
        if (rst) begin
            tail <= 0;
            head <= 0;
            count <= 0;
        end else begin
            if ((push && (count != (1 << AW)))) begin
                data[tail] <= din;
                tail <= (tail + 1);
                if ((pop && (count != 0))) begin
                    head <= (head + 1);
                end else begin
                    count <= (count + 1);
                end
            end else begin
                if ((pop && (count != 0))) begin
                    head <= (head + 1);
                    count <= (count - 1);
                end
            end
        end
    end
endmodule

