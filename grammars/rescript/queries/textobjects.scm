; Queries for nvim-treesitter/nvim-treesitter-textobjects
;--------------------------------------------------------

; Classes (modules)
;------------------

(module_binding definition: ((_) @class.inner)) @class.outer

; Blocks
;-------

(block (_) @block.inner) @block.outer

; Functions
;----------

(function body: (_) @function.inner) @function.outer

; Calls
;------

(call_expression arguments: ((_) @call.inner)) @call.outer

; Comments
;---------

(comment) @comment.outer

; Parameters
;-----------

(function parameter: (_) @parameter.inner @parameter.outer)

(formal_parameters
  "," @_formal_parameters_start
  . (_) @parameter.inner
  (#make-range! "parameter.outer" @_formal_parameters_start @parameter.inner))
(formal_parameters
  . (_) @parameter.inner
  . ","? @_formal_parameters_end
  (#make-range! "parameter.outer" @parameter.inner @_formal_parameters_end))

(arguments
  "," @_arguments_start
  . (_) @parameter.inner
  (#make-range! "parameter.outer" @_arguments_start @parameter.inner))
(arguments
  . (_) @parameter.inner
  . ","? @_arguments_end
  (#make-range! "parameter.outer" @parameter.inner @_arguments_end))

(function_type_parameters
  "," @_function_type_parameters_start
  . (_) @parameter.inner
  (#make-range! "parameter.outer" @_function_type_parameters_start @parameter.inner))
(function_type_parameters
  . (_) @parameter.inner
  . ","? @_function_type_parameters_end
  (#make-range! "parameter.outer" @parameter.inner @_function_type_parameters_end))

(functor_parameters
  "," @_functor_parameters_start
  . (_) @parameter.inner
  (#make-range! "parameter.outer" @_functor_parameters_start @parameter.inner))
(functor_parameters
  . (_) @parameter.inner
  . ","? @_functor_parameters_end
  (#make-range! "parameter.outer" @parameter.inner @_functor_parameters_end))

(type_parameters
  "," @_type_parameters_start
  . (_) @parameter.inner
  (#make-range! "parameter.outer" @_type_parameters_start @parameter.inner))
(type_parameters
  . (_) @parameter.inner
  . ","? @_type_parameters_end
  (#make-range! "parameter.outer" @parameter.inner @_type_parameters_end))

(type_arguments
  "," @_type_arguments_start
  . (_) @parameter.inner
  (#make-range! "parameter.outer" @_type_arguments_start @parameter.inner))
(type_arguments
  . (_) @parameter.inner
  . ","? @_type_arguments_end
  (#make-range! "parameter.outer" @parameter.inner @_type_arguments_end))

(decorator_arguments
  "," @_decorator_arguments_start
  . (_) @parameter.inner
  (#make-range! "parameter.outer" @_decorator_arguments_start @parameter.inner))
(decorator_arguments
  . (_) @parameter.inner
  . ","? @_arguments_end
  (#make-range! "parameter.outer" @parameter.inner @_arguments_end))

(variant_parameters
  "," @_variant_parameters_start
  . (_) @parameter.inner
  (#make-range! "parameter.outer" @_variant_parameters_start @parameter.inner))
(variant_parameters
  . (_) @parameter.inner
  . ","? @_variant_parameters_end
  (#make-range! "parameter.outer" @parameter.inner @_variant_parameters_end))

(polyvar_parameters
  "," @_polyvar_parameters_start
  . (_) @parameter.inner
  (#make-range! "parameter.outer" @_polyvar_parameters_start @parameter.inner))
(polyvar_parameters
  . (_) @parameter.inner
  . ","? @_polyvar_parameters_end
  (#make-range! "parameter.outer" @parameter.inner @_polyvar_parameters_end))

