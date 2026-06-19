module Priority (
    input wire [(3)-1:0] req,
    output wire [(2)-1:0] grant
);
    assign grant = ((((req & 'b100) == 'b100)) ? ('b11) : ((((req & 'b110) == 'b010)) ? ('b10) : (((req == 'b001)) ? ('b01) : ('b00))));
endmodule

