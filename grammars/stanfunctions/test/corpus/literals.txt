================================================================================
Numeric literals
================================================================================


transformed data {
    int a = 0;
    int b = 10_000;
    real c = .12;
    real d = 1.;
    real e = 1.002_345;
    real f = .99_99_99e+1_25;
    real g = 1_100_100.0_345;
    complex h = 0i;
    complex j = 100_00i;
    complex k = 1.0_0i;
    complex l = 1.0_0e+1_00i;
    complex m = 0.i;
    complex n = .1i;
}

--------------------------------------------------------------------------------

(program
  (transformed_data
    (top_var_decl
      (top_var_type
        (int_type))
      (identifier)
      (integer_literal))
    (top_var_decl
      (top_var_type
        (int_type))
      (identifier)
      (integer_literal))
    (top_var_decl
      (top_var_type
        (real_type))
      (identifier)
      (real_literal
        (integer_literal)))
    (top_var_decl
      (top_var_type
        (real_type))
      (identifier)
      (real_literal
        (integer_literal)))
    (top_var_decl
      (top_var_type
        (real_type))
      (identifier)
      (real_literal
        (integer_literal)
        (integer_literal)))
    (top_var_decl
      (top_var_type
        (real_type))
      (identifier)
      (real_literal
        (integer_literal)
        (exp_literal
          (integer_literal))))
    (top_var_decl
      (top_var_type
        (real_type))
      (identifier)
      (real_literal
        (integer_literal)
        (integer_literal)))
    (top_var_decl
      (top_var_type
        (complex_type))
      (identifier)
      (imag_literal
        (integer_literal)))
    (top_var_decl
      (top_var_type
        (complex_type))
      (identifier)
      (imag_literal
        (integer_literal)))
    (top_var_decl
      (top_var_type
        (complex_type))
      (identifier)
      (imag_literal
        (real_literal
          (integer_literal)
          (integer_literal))))
    (top_var_decl
      (top_var_type
        (complex_type))
      (identifier)
      (imag_literal
        (real_literal
          (integer_literal)
          (integer_literal)
          (exp_literal
            (integer_literal)))))
    (top_var_decl
      (top_var_type
        (complex_type))
      (identifier)
      (imag_literal
        (real_literal
          (integer_literal))))
    (top_var_decl
      (top_var_type
        (complex_type))
      (identifier)
      (imag_literal
        (real_literal
          (integer_literal))))))

================================================================================
String literals
================================================================================

generated quantities {
   print("hello world");
   reject("abzxdfd e324321 jfe. !#@#R$%$@()+(+()''");
}

--------------------------------------------------------------------------------

(program
  (generated_quantities
    (print_statement
      (string_literal))
    (reject_statement
      (string_literal))))
