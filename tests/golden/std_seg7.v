module Seg7 (
    input wire [(4)-1:0] digit,
    output wire [(7)-1:0] seg
);
    assign seg = (((digit == 0)) ? ('h3F) : (((digit == 1)) ? ('h06) : (((digit == 2)) ? ('h5B) : (((digit == 3)) ? ('h4F) : (((digit == 4)) ? ('h66) : (((digit == 5)) ? ('h6D) : (((digit == 6)) ? ('h7D) : (((digit == 7)) ? ('h07) : (((digit == 8)) ? ('h7F) : (((digit == 9)) ? ('h6F) : ('h00)))))))))));
endmodule

