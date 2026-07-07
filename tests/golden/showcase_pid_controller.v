module PidController (
    input wire clk,
    input wire rst,
    input wire signed [(8)-1:0] setpoint,
    input wire signed [(8)-1:0] measured,
    output wire signed [(8)-1:0] control,
    output wire saturated
);
    reg signed [(16)-1:0] integral;
    reg signed [(8)-1:0] prev_error;
    wire signed [(9)-1:0] error;
    wire signed [(10)-1:0] p_term;
    wire signed [(10)-1:0] d_diff;
    wire signed [(16)-1:0] total;
    assign error = ((setpoint) - (measured))[(9)-1:0];
    assign p_term = ((error) + (error))[(10)-1:0];
    assign d_diff = ((error) - (prev_error))[(10)-1:0];
    assign total = (((p_term) + integral)[(16)-1:0] + (d_diff))[(16)-1:0];
    assign control = (((-128) < ((total < 127) ? (total) : (127))) ? (((total < 127) ? (total) : (127))) : ((-128)))[(8)-1:0];
    assign saturated = ((total < (-128)) || (total > 127));
    always @(posedge clk) begin
        if (rst) begin
            integral <= 0;
            prev_error <= 0;
        end else begin
            integral <= integral;
            integral <= (integral + (error))[(16)-1:0];
            prev_error <= error[(8)-1:0];
        end
    end
endmodule

