; lotml highlights, variant B. Generated from harness/lotml_harness/lang/grammar.py; do not edit by hand.

["and" "assert" "break" "case" "continue" "dyn" "elif" "else" "fail" "fn" "for" "from" "if" "impl" "import" "in" "inout" "is" "lambda" "match" "not" "or" "pass" "return" "sink" "test" "trait" "type" "var" "while"] @keyword
["None" "True" "False"] @constant.builtin

(fn_head (name) @function)
(type_def (name) @type)
(type (name) @type)
(param (name) @variable.parameter)

(string) @string
(number) @number
(comment) @comment
