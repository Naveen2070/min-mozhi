module villakku (
    input wire manni,
    input wire miill,
    output wire olli
);
    reg sutar;
    assign olli = sutar;
    always @(posedge manni) begin
        if (miill) begin
            sutar <= 0;
        end else begin
            sutar <= (!sutar);
        end
    end
endmodule

