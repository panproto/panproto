# Ansible for DevOps examples

Source: <https://github.com/geerlingguy/ansible-for-devops>, branch `master`,
commit `86e1bf80098239cd995d54217585e8d61117d2e7`, licensed under the MIT
License (`LICENSE`).

Vendored, unchanged, as `upstream/<class>/<path with / as _>`, the class
being the one the reader is given:

- playbooks: `first-ansible-playbook/playbook.yml`,
  `includes/provisioning/playbook.yml`, `lamp-infrastructure/provision.yml`,
  `lamp-infrastructure/configure.yml`, `docker/main.yml`,
  `jenkins/provision.yml`, `lamp-infrastructure/playbooks/db/main.yml`,
  `deployments/playbooks/provision.yml`, `https-letsencrypt/main.yml`
- task files: `nodejs-role/roles/nodejs/tasks/main.yml`,
  `includes/provisioning/tasks/php.yml`,
  `includes/provisioning/tasks/apache.yml`,
  `includes/provisioning/handlers/handlers.yml`,
  `https-self-signed/provisioning/tasks/self-signed-cert.yml`
- variables files: `drupal/provisioning/vars.yml`,
  `includes/provisioning/vars.yml`, `kubernetes/vars/main.yml`,
  `docker-hubot/roles/hubot-slack/defaults/main.yml`,
  `lamp-infrastructure/playbooks/db/vars.yml`

The files under `invalid/` break one rule each; `MANIFEST` lists the class,
the rule and the refusal expected. `conformance_ansible.rs` runs every
case; the cases the reader does not pass are listed with their limit in
`../ansible-limits.txt`.
