# Ansible

An Ansible playbook says which hosts to configure and what to run on them: plays hold roles, tasks and handlers, a task calls one module with its arguments, and a task that changes something notifies a handler by name. Reading a playbook into a panproto schema makes those parts and the names that join them a graph, so a change to a role's variables or a renamed handler shows up as a change to the schema. This page states which Ansible files panproto reads, how a file becomes a schema, what the reader refuses, what the writer prints, and the laws the two satisfy.

## Standard and shapes

The reader follows the [playbook keywords](https://docs.ansible.com/ansible/latest/reference_appendices/playbooks_keywords.html) of [ansible-core](https://github.com/ansible/ansible) 2.22.0, transcribed from the field attributes of its `Play`, `Block`, `Task`, `Handler` and `Role` classes, and its rule for variable names (`validate_variable_name`: an ASCII identifier other than `true`, `false`, `none`, `True`, `False`, `None` and `not`). A file does not say what it is, so the caller names its class: a playbook, a task file (a role's `tasks/main.yml` or `handlers/main.yml`, or any file an `import_tasks` names), or a variables file (a role's `defaults/main.yml` or `vars/main.yml`, a `group_vars` or `host_vars` file). A file's syntax is read as a tree and its meaning as a tree too: the `ansible` protocol declares a tree syntactic and a tree semantic shape, and the `notify` edges are references within it. The reader is [`read_ansible`](https://docs.rs/panproto-io/latest/panproto_io/config/ansible/fn.read_ansible.html) in `panproto-io` (feature `tree-sitter`). It reads no YAML itself: the file goes through the YAML route of the [text formats](text-formats.md) (YAML 1.2.2, one document, the core schema), and every refusal of that route is the reader's refusal too. `panproto-protocols` does not parse Ansible files, and its schema dispatches refuse `ansible` and name this reader.

ansible-core reads YAML 1.1, which gives some plain scalars another value than YAML 1.2 does. The reader reads a plain `yes`, `on`, `no` or `off` (in any of their three capitalisations) as the boolean ansible-core reads, and a plain octal such as `0755` as its value (493); a plain scalar YAML 1.1 reads as another number or a date (`0b11`, `1_000`, `1:30`, `2001-12-14`) or as a string where YAML 1.2 reads a number (`0o644`) is refused, since quoting it says which value is meant. A mapping key is read as YAML 1.2 reads it.

| Supported | Read as |
|---|---|
| A playbook: a sequence of plays and `import_playbook` entries | A `playbook` vertex `/` with an `entry` edge to each `play` or `playbook-import` |
| A play's keywords, `vars`, `roles`, `pre_tasks`, `tasks`, `post_tasks`, `handlers` | A `play` vertex; its keywords as constraints, its variables, roles and tasks below edges |
| A role entry: a name, or a mapping with `role` (or `name`), keywords, `vars` and parameters | A `role` vertex; the role's name and keywords as constraints, its parameters as variables below `param` edges |
| A block with `block`, `rescue`, `always` and keywords | A `block` vertex with its tasks below `block`, `rescue` and `always` edges |
| A task or handler: one module key with its arguments, keywords, `with_<lookup>` | A `task` or `handler` vertex; the module and arguments as `module` and `arguments`, the lookup as `with` and `with_terms` |
| `vars` mappings and a variables file | A `variable` vertex per variable below a `var` edge named by the variable, its value as `value` |
| `!vault` and `!unsafe` scalars | The JSON forms ansible-core writes, `{"__ansible_vault": …}` and `{"__ansible_unsafe": …}` |

| Not supported | Refusal | Reason |
|---|---|---|
| The `action` and `local_action` keywords | `Unsupported` | They name the module inside a string; write the module as the task's key |
| A plain scalar YAML 1.1 reads as another number or a date | `Yaml11Scalar` | The YAML 1.2 route reads it as something else; quote it |
| Other local tags | `UnknownTag` | ansible-core defines only `!vault` and `!unsafe` |
| Role metadata (`meta/main.yml`), `requirements.yml`, `argument_specs`, inventories, `ansible.cfg` | not a file class | Their shapes are not playbook keywords |
| What a module, lookup, filter or template does | not read | They are plugins resolved when a play runs; their arguments are kept as written |

## The Ansible schema

The root vertex `/` has the kind of the file's class: `playbook`, `task-file` or `vars-file`. Every other vertex's id is `/` followed by the path of its node (`/[0]` for the first play, `/[0].tasks[1]` for its second task, `/[0].vars.port` for a variable of the play), and each keyword is a constraint of its vertex whose value is the keyword's value as canonical JSON text. A task's module is the one key of the task that is not one of its keywords and does not begin with `with_`: the reader keeps its name as `module` and its value (a mapping, a free-form string, or null) as `arguments`, and it does not resolve the name, since a module is a plugin found when a play runs. A `with_<lookup>` loop is kept as `with` (the lookup) and `with_terms`, and the lookup is not resolved either.

`notify` names resolve within a play. Before reading a play's tasks the reader collects its handlers, by `name` and by each `listen` topic, and each `notify` name of a task or block becomes a `notify` edge to every handler that answers to it, in the order the handlers are declared, added after the play's tasks are read. A name no handler of the play answers to (a handler of a role, or any name in a task file) is reported. So are the other references a file makes to things outside it, in the reading's [`external`](https://docs.rs/panproto-io/latest/panproto_io/config/ansible/struct.AnsibleReading.html) list, in document order: each role a play names, each `vars_files` entry, each `import_playbook`, and the file or role of an `include_tasks`, `import_tasks`, `include_role`, `import_role` or `include_vars` task (with or without the `ansible.builtin.` or `ansible.legacy.` prefix). Validation keeps every kept keyword as an annotation and evaluates no condition or template.

## What the reader refuses

Every refusal is an [`AnsibleError`](https://docs.rs/panproto-io/latest/panproto_io/config/ansible/struct.AnsibleError.html) carrying the path of the field at fault, the byte offset at which its value starts, and an [`AnsibleErrorKind`](https://docs.rs/panproto-io/latest/panproto_io/config/ansible/enum.AnsibleErrorKind.html). The reader returns the first fault in this order. First the route (`Syntax`), then, in document order, every tag other than `!vault` and `!unsafe` (`UnknownTag`) and every divergent YAML 1.1 scalar (`Yaml11Scalar`). Then the file:

- a playbook is a sequence (`WrongType`) of at least one entry (`Invalid`); a task file is a sequence; a variables file is a mapping or null;
- an entry with an `import_playbook` key has only `import_playbook` (or `ansible.builtin.import_playbook`), `name`, `vars`, `when` and `tags`, one import, a string `name` and a string import;
- a play has only play keywords (`UnknownField`) and `hosts` (`Missing`); then each keyword in source order (a string `name`, a sequence of strings or sequences of strings for `vars_files`); then `vars`, each `roles` entry, and the sections `pre_tasks`, `tasks`, `post_tasks` and `handlers`, each a sequence;
- a role entry is a string or a mapping naming its role by `role` or `name` (`Missing("role")`); each key that is neither a role keyword nor `vars` is a parameter whose name must be a variable name; then its keywords in source order and its `vars`;
- an item of a section is a mapping; with a `block` key it is a block, which has only block keywords, then its keywords, `vars`, `notify`, and its `block`, `rescue` and `always` sequences; otherwise a task, whose keys, in source order, are checked for `action` and `local_action` (`Unsupported`), a second loop (`loop` or `with_*`, `Invalid`) and a second module (`Invalid`), and which must have a module (`Missing`); then each keyword in source order, then `vars` and `notify`.

A keyword's value is checked as ansible-core types it where its shape matters to the schema: `name` a string, `notify` and `listen` a string or a sequence of strings, `loop_control` a mapping, `register` a variable name (or a mapping of them), and `vars` a mapping (or null) whose keys are variable names (`VariableName`). The reader does not check the other keywords' values (a `when` condition, a `become` flag, a `serial` batch), which ansible-core checks when it runs the play, nor whether a module, lookup or role exists.

## Canonical output

[`write_ansible`](https://docs.rs/panproto-io/latest/panproto_io/config/ansible/fn.write_ansible.html) writes an `ansible` schema as the canonical file: one line of JSON with no whitespace between tokens, ended by a line feed, which YAML 1.2 reads as the same document. A playbook or task file is a sequence and a variables file a mapping (`{}` when it declares nothing). A play, import, block, task or handler writes its kept keywords in source order (a task's module as its key with its arguments, a lookup loop as its `with_` key), then its `vars`; a play then writes `roles`, `pre_tasks`, `tasks`, `post_tasks` and `handlers`, and a block `block`, `rescue` and `always`, each that holds anything; a role writes `role`, its keywords, its parameters and its `vars`; an import writes its playbook under `import_playbook`. The writer refuses, with an [`AnsibleWriteError`](https://docs.rs/panproto-io/latest/panproto_io/config/ansible/enum.AnsibleWriteError.html), a schema without a file vertex, a variable without a value, and a kept constraint that is not JSON. The reader claims no format preservation: the canonical text keeps every keyword and value but not comments, layout, quoting or YAML 1.1 spellings.

## Conformance corpus

The corpus under `crates/panproto-io/tests/fixtures/conformance/ansible` holds 19 files vendored unchanged from [ansible-for-devops](https://github.com/geerlingguy/ansible-for-devops) at commit `86e1bf80` (MIT License; `UPSTREAM.md` names each file and its class): nine playbooks, five task files and five variables files. Each must read under its class, and its canonical text must read back as the same schema and write as the same text. The Ansible project publishes no suite of invalid files, so 16 refusal cases break one rule each, most of them built from the book's first playbook, and `MANIFEST` lists each case's class, rule, path and refusal (`conformance_ansible.rs`). All 35 cases pass, and the corpus declares no limit.

## A file reads as the page defines it {#L-ansible-1}

### Statement

For a document $d$ of class $c$ in the bounded scope, write $R(d, c)$ for `read(d, c)` and $Y(d)$ for its block YAML text `render_yaml(d)`. When $R(d, c)$ is a reading, `read_ansible` reads $Y(d)$ under $c$ as a schema with exactly the vertices and kinds of $R(d, c)$, the out-edges of each vertex in the same order with the same kinds, targets and names, the same kept constraints in the same order with the same JSON texts, and the same external references in the same order. When $R(d, c)$ refuses at path $p$ with class $k$, `read_ansible` refuses $Y(d)$ at $p$ with class $k$.

### Outside the domain

The reference reads data trees whose strings $Y$ writes quoted, so plain YAML 1.1 scalars, tags and floats are checked by the fixed tests and the corpus. A refusal's byte offset and the message of an `Invalid` refusal are not compared; the path and class are. Refusals of the YAML route are the subject of [Text formats](text-formats.md).

### Definitions used

$R$ applies the rules of the two sections above in their order. The scope is a base playbook (one play on all hosts with a handler `restart`) whose play holds no task, one of 17 tasks, or an ordered pair of them (a named task; tasks notifying the handler and nothing; a loop; a lookup loop; two loops; two modules; no module; `action`; a block with a rescue that notifies; a block with an unknown key; valid and invalid task variables; an invalid `register`; a task include; a name that is not a string; a role import), together with 23 envelopes of the three classes (a play without hosts or with an unknown key, play variables and variables files, roles in every form and two invalid ones, playbook imports, an empty playbook, a playbook that is not a sequence, handlers answering by `listen` and inside a block, a section that is not a sequence, task files, variables files with a reserved name, null and a sequence, a play name that is not a string, and a `register` mapping): $1 + 17 + 17^2 + 23 = 330$ documents.

### Proof

1. Production reads $Y(d)$ through the YAML route, which reads a block document whose keys and strings are double-quoted and whose other scalars are integers, booleans and nulls as the tree $d$ (block reading is [L-text-formats-4](text-formats.md#L-text-formats-4)); a quoted string is not plain, so the YAML 1.1 step leaves it unchanged.
2. Production checks the same conditions as $R$ in the same order, so the first condition that fails is the same, at the same path, with the same class.
3. On a document both accept, both create the same vertices with the same ids, add each edge once in the same order (the `notify` edges of a play after its tasks), keep each keyword's JSON text in source order, and report the same external references in document order, so the readings agree.
4. The exhaustive differential checks the agreement on all 330 documents, of which 100 are accepted (9 of the 17 tasks are accepted, so $1 + 9 + 9^2 = 91$ playbooks, and 9 envelopes), and the property differential on generated playbooks of nested blocks.

### Executable counterpart

- Law ID: `ansible.reading`
- Reference: `panproto_reference::pages::ansible::read`
- Reference: `panproto_reference::pages::ansible::render_yaml`
- Enumerator: `panproto_reference::enumerate::ansible::documents; bound: documents=330`
- Exhaustive differential test: `ansible_reading_and_canonical_writing_match_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_ansible --no-tests=fail -E 'test(=ansible_reading_and_canonical_writing_match_the_reference_on_every_bounded_document)'`
- Property differential test: `randomized_playbooks_read_and_write_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_ansible --no-tests=fail -E 'test(=randomized_playbooks_read_and_write_as_the_reference)'`
- Reference test: `a_playbook_reads_as_its_plays_tasks_and_handlers; command: cargo nextest run -p panproto-reference --test ansible --no-tests=fail -E 'test(=a_playbook_reads_as_its_plays_tasks_and_handlers)'`
- Reference test: `ansible_refusals_name_the_field_and_the_rule; command: cargo nextest run -p panproto-reference --test ansible --no-tests=fail -E 'test(=ansible_refusals_name_the_field_and_the_rule)'`
- Randomized input scope: up to two tasks, each a module call (`ping`, `copy`, `include_tasks` or `ansible.builtin.debug`) notifying `restart`, `h` or `missing`, with one variable named from `x`, `y`, `not` and `_z` and possibly a lookup loop, or a block of one or two such tasks with up to one rescue task, nested two deep.
- File: `crates/panproto-io/src/config/ansible.rs`
- File: `crates/panproto-io/src/config/tree.rs`
- File: `crates/panproto-reference/src/pages/ansible.rs`

## The writer prints the canonical file, which reads back as the same schema {#L-ansible-2}

### Statement

For a document $d$ of class $c$ in the bounded scope that $R$ accepts, write $C(d, c)$ for `canonical_text(d, c)`. `write_ansible` writes the schema `read_ansible` reads from $Y(d)$ under $c$ as exactly $C(d, c)$, and `read_ansible` reads $C(d, c)$ under $c$ as a schema with the same vertices, edges and kept constraints.

### Outside the domain

The external references are not compared on the canonical text, whose paths may differ (a role named by a string is written as a mapping, so its path gains `.role`). Schemas that no file reads as are refused by the writer; the reference, which writes from documents, does not model them.

### Definitions used

$C(d, c)$ is the compact JSON of $d$ with its keys reordered as the canonical-output section states, every string escaped as RFC 8259 requires, followed by a line feed.

### Proof

1. Production writes each keyword from its kept JSON text, which by [L-ansible-1](#L-ansible-1) is the compact JSON of its value in $d$, writes variables, roles and tasks from the edges, which hold them in source order, and orders the keys as $C$ does.
2. $C(d, c)$ is a JSON document that YAML 1.2 reads as the tree of $d$ with its keys reordered, and none of its strings is plain. The reader adds edges in an order that does not depend on the order of a play's or a block's keys (variables, then roles, then the sections in a fixed order, then `notify`), so it reads $C(d, c)$ as the same schema.
3. The exhaustive differential checks both equalities on all 100 accepted documents, and the property differential on generated ones.

### Executable counterpart

- Law ID: `ansible.canonical-writing`
- Reference: `panproto_reference::pages::ansible::canonical_text`
- Enumerator: `panproto_reference::enumerate::ansible::documents; bound: documents=330`
- Exhaustive differential test: `ansible_reading_and_canonical_writing_match_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_ansible --no-tests=fail -E 'test(=ansible_reading_and_canonical_writing_match_the_reference_on_every_bounded_document)'`
- Property differential test: `randomized_playbooks_read_and_write_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_ansible --no-tests=fail -E 'test(=randomized_playbooks_read_and_write_as_the_reference)'`
- Reference test: `the_canonical_file_writes_keywords_then_variables_then_sections; command: cargo nextest run -p panproto-reference --test ansible --no-tests=fail -E 'test(=the_canonical_file_writes_keywords_then_variables_then_sections)'`
- Reference test: `the_ansible_document_scope_has_its_closed_form_count; command: cargo nextest run -p panproto-reference --test ansible --no-tests=fail -E 'test(=the_ansible_document_scope_has_its_closed_form_count)'`
- File: `crates/panproto-io/src/config/ansible.rs`
- File: `crates/panproto-reference/src/pages/ansible.rs`
- File: `crates/panproto-reference/src/enumerate/ansible.rs`
