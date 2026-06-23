module ennkaatti (
    input wire [(4)-1:0] ilakkam,
    output wire [(7)-1:0] kaatsi
);
    assign kaatsi = (((ilakkam == 0)) ? ('h3F) : (((ilakkam == 1)) ? ('h06) : (((ilakkam == 2)) ? ('h5B) : (((ilakkam == 3)) ? ('h4F) : (((ilakkam == 4)) ? ('h66) : (((ilakkam == 5)) ? ('h6D) : (((ilakkam == 6)) ? ('h7D) : (((ilakkam == 7)) ? ('h07) : (((ilakkam == 8)) ? ('h7F) : (((ilakkam == 9)) ? ('h6F) : ('h00)))))))))));
endmodule

