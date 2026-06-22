module nilaippatuththi #(
    parameter akalam = 3,
    parameter urruthi = 4
) (
    input wire katikai,
    input wire miill,
    input wire muulam,
    output wire thellivu
);
    reg oththumun;
    reg oththupin;
    reg [(akalam)-1:0] ennnni;
    reg eerrpu;
    assign thellivu = eerrpu;
    always @(posedge katikai) begin
        if (miill) begin
            oththumun <= 0;
            oththupin <= 0;
            ennnni <= 0;
            eerrpu <= 0;
        end else begin
            oththumun <= muulam;
            oththupin <= oththumun;
            if ((oththupin == eerrpu)) begin
                ennnni <= 0;
            end else begin
                if ((ennnni == urruthi)) begin
                    eerrpu <= oththupin;
                    ennnni <= 0;
                end else begin
                    ennnni <= (ennnni + 1);
                end
            end
        end
    end
endmodule

