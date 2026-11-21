// @dialect vjass


integer a = 22

globals 
    integer b = 333
endglobals

//! zinc
library EmbeddedZinc {
    /* handed to the Zinc parser */
    function Twice(integer x) -> integer { return x * 2; }
}
//! endzinc
