(namespace
  (name) @name) @definition.namespace

(entity_declaration
  (identifier_list
    (identifier) @name)) @definition.type

(action_declaration
  (action_name_list
    (identifier) @name)) @definition.function

(action_declaration
  (action_name_list
    (string) @name)) @definition.function

(common_type_declaration
  (identifier) @name) @definition.type
