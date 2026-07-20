module PidController (
    input wire clk,
    input wire rst,
    input wire signed [(8)-1:0] setpoint,
    input wire signed [(8)-1:0] measured,
    output wire signed [(8)-1:0] control,
    output wire saturated
);
    wire signed [9:0] __mimz_sub_1;
    assign __mimz_sub_1 = ((setpoint) - (measured));
    wire signed [10:0] __mimz_sub_2;
    assign __mimz_sub_2 = ((error) + (error));
    wire signed [10:0] __mimz_sub_3;
    assign __mimz_sub_3 = ((error) - (prev_error));
    wire signed [16:0] __mimz_sub_4;
    assign __mimz_sub_4 = ((p_term) + integral);
    wire signed [16:0] __mimz_sub_5;
    assign __mimz_sub_5 = (__mimz_sub_4[(16)-1:0] + (d_diff));
    wire signed [16:0] __mimz_sub_6;
    assign __mimz_sub_6 = (integral + (error));
    reg signed [(16)-1:0] integral;
    reg signed [(8)-1:0] prev_error;
    wire signed [(9)-1:0] error;
    wire signed [(10)-1:0] p_term;
    wire signed [(10)-1:0] d_diff;
    wire signed [(16)-1:0] total;
    assign error = __mimz_sub_1[(9)-1:0];
    assign p_term = __mimz_sub_2[(10)-1:0];
    assign d_diff = __mimz_sub_3[(10)-1:0];
    assign total = __mimz_sub_5[(16)-1:0];
    assign control = (((-128) < ((total < 127) ? (total) : (127))) ? (((total < 127) ? (total) : (127))) : ((-128)))[(8)-1:0];
    assign saturated = ((total < (-128)) || (total > 127));
    always @(posedge clk) begin
        if (rst) begin
            integral <= 0;
            prev_error <= 0;
        end else begin
            integral <= integral;
            integral <= __mimz_sub_6[(16)-1:0];
            prev_error <= error[(8)-1:0];
        end
    end
endmodule

