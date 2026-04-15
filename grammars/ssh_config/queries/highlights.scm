; Literals

(string) @string

(pattern) @string.regexp

(token) @string.special.symbol

[
  (number)
  (bytes)
  (time)
] @number

[
  (kex)
  (mac)
  (cipher)
  (key_sig)
] @string.special

[
  ; generic
  "yes" "no"
  "ask" "auto"
  "none" "any"
  ; CanonicalizeHostname
  "always"
  ; ControlMaster
  "autoask"
  ; FingerprintHash
  "md5" "sha256"
  ; PubkeyAuthentication
  "unbound" "host-bound"
  ; RequestTTY
  "force"
  ; SessionType
  "subsystem" "default"
  ; StrictHostKeyChecking
  "accept-new" "off"
  ; Tunnel
  "point-to-point" "ethernet"
  (ipqos)
  (verbosity)
  (facility)
  (authentication)
] @constant.builtin

(uri) @markup.link.url

; Keywords

[ "Host" "Match" ] @module

(parameter keyword: _ @keyword)

(host_declaration argument: _ @tag)

(match_declaration
  (condition criteria: _ @variable.parameter))

"all" @variable.parameter

; Misc

[
  "SSH_AUTH_SOCK"
  (variable)
] @constant

(comment) @comment

; Punctuation

[ "${" "}" ] @punctuation.special

[ "\"" "," ":" "@" ] @punctuation.delimiter

[ "=" "!" "+" "-" "^" ] @operator

[ "*" "?" ] @character.special
