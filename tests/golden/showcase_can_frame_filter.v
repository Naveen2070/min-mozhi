module CanFrameFilter (
    input wire clk,
    input wire rst,
    input wire [(2)-1:0] kind,
    input wire valid,
    output wire filter_out,
    output wire [(2)-1:0] frame_type,
    output wire [(4)-1:0] match_count
);
    reg [(4)-1:0] data_cnt;
    reg [(4)-1:0] remote_cnt;
    reg [(4)-1:0] err_cnt;
    reg filter_out_reg;
    reg [(2)-1:0] frame_type_reg;
    reg [(4)-1:0] match_count_reg;
    wire accept;
    // NOTE (BUG-65, docs/audit/bugs.md): the `initial` register-init line(s) below are simulation/FPGA-only - an ASIC flow has no defined power-on default and will not honor them. The synchronous reset below still applies regardless.
    initial data_cnt = 0;
    initial remote_cnt = 0;
    initial err_cnt = 0;
    initial filter_out_reg = 0;
    initial frame_type_reg = 0;
    initial match_count_reg = 0;
    assign accept = (((kind == 0)) ? (1) : (((kind == 1)) ? (1) : (0)));
    assign filter_out = filter_out_reg;
    assign frame_type = frame_type_reg;
    assign match_count = match_count_reg;
    always @(posedge clk) begin
        if (rst) begin
            filter_out_reg <= 0;
            frame_type_reg <= 0;
            match_count_reg <= 0;
            data_cnt <= 0;
            remote_cnt <= 0;
            err_cnt <= 0;
        end else begin
            filter_out_reg <= 0;
            frame_type_reg <= 0;
            if (valid) begin
                frame_type_reg <= kind;
                if (accept) begin
                    filter_out_reg <= 1;
                    match_count_reg <= (match_count_reg + 4'd1);
                end
                if ((kind == 0)) begin
                    data_cnt <= (data_cnt + 4'd1);
                end
                if ((kind == 1)) begin
                    remote_cnt <= (remote_cnt + 4'd1);
                end
                if ((kind == 2)) begin
                    err_cnt <= (err_cnt + 4'd1);
                end
            end
        end
    end
endmodule

