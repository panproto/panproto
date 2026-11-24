# AWS CloudFormation sample templates

Source: <https://github.com/aws-cloudformation/aws-cloudformation-templates>,
branch `main`, commit `a0f43bc6d20813052892546f445037cf84c75b54`
(2025-10-13), licensed under the Apache License 2.0 (`LICENSE`, with the
repository's `NOTICE`).

Vendored, unchanged, as `upstream/<directory>_<file name>`:

- `CloudFormation/fn-foreach-ddb.json`
- `CloudWatch/CloudWatch_Dashboard_NAT_FlowLogs.json` and `.yaml`
- `DynamoDB/DynamoDB_Table.json` and `.yaml`
- `EC2/InstanceWithCfnInit.json` and `.yaml`
- `EC2/SingleENIwithMultipleEIPs.json` and `.yaml`
- `RDS/RDS_PIOPS.json` and `.yaml`
- `RDS/RDS_Snapshot_On_Delete.json` and `.yaml`
- `RDS/RDS_with_DBParameterGroup.json` and `.yaml`
- `SNS/SNSTopic.json` and `.yaml`

The files under `invalid/` are `SNS/SNSTopic.yaml` (and, for the duplicate
key, `SNS/SNSTopic.json`) changed to break one rule each; `MANIFEST` lists
the rule and the refusal expected, and lists `fn-foreach-ddb.json`, which
uses the `AWS::LanguageExtensions` transform, with its refusal.
`conformance_cloudformation.rs` runs every case; the cases the reader does
not pass are listed with their limit in `../cloudformation-limits.txt`.
