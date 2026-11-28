  `ifndef SYNTHESIS
  function automatic string syl$hex(input string bits, input bit upper);
    string digits, result;
    int digit;
    byte ch;
    bit known, all_z;
    digits = upper ? "0123456789ABCDEF" : "0123456789abcdef";
    result = "";
    digit = 0;
    known = 1;
    all_z = 1;
    for (int i = 0; i < bits.len(); i++) begin
      ch = bits[i];
      digit = (digit << 1) | int'(ch == "1");
      known &= (ch == "0" || ch == "1");
      all_z &= (ch == "z");
      // Start with a partial digit when the width is not a multiple of four.
      if ((bits.len() - i - 1) % 4 == 0) begin
        if (known) ch = digits[digit];
        else if (all_z) ch = upper ? "Z" : "z";
        else ch = upper ? "X" : "x";
        result = {result, ch};
        digit = 0;
        known = 1;
        all_z = 1;
      end
    end
    return result;
  endfunction
  `endif
