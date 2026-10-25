# IAM for operating Wartide

- `wartide-terraform-policy.json`: least-privilege policy for the `wartide-terraform` role that the `wartide`
  AWS CLI profile assumes to run Terraform. Allows regional (us-east-1) management of the services Terraform uses,
  Route53, Secrets Manager only for `wartide-*` secrets, IAM only for `wartide-*` roles/users/policies plus
  service-linked roles, and the state bucket. Explicitly denies changing the `wartide-terraform` role itself and
  attaching `AdministratorAccess` to anything.
  Principals Terraform creates (`wartide-*` roles and users) can only be created or given inline/managed policies
  when they carry the `wartide-boundary` permissions boundary; the policy denies changing or deleting either
  managed policy and denies setting or removing any permissions boundary.
- `wartide-boundary-policy.json`: the permissions boundary (an allow-list) attached to every role and user
  Terraform creates (task, task-exec, deploy role, SES SMTP user): ECR pull/push on `wartide-*` repositories,
  ECS task-definition registration and service update, app log writes, `wartide-*` secrets read, SES send,
  `sts:GetCallerIdentity`, and `iam:PassRole` to ECS tasks. Nothing outside this list can ever be granted to them.
- `wartide-terraform-trust.json`: trust policy template; `__ADMIN_PRINCIPAL_ARN__` is substituted by the script
  (only that principal may assume the role).

Create or update everything with the script (run as an admin profile, not via the role; idempotent, prints each
AWS command, creates/updates `wartide-boundary` first and then the operator policy as a new default version only
when the JSON changed, deleting the oldest non-default version to stay within IAM's limit of 5, and attaches the
boundary to existing `wartide-<env>-*` roles and users because the operator role itself may not):

```sh
AWS_PROFILE=default ADMIN_PRINCIPAL_ARN=arn:aws:iam::<acct>:user/<admin> scripts/aws-bootstrap.sh operator-role
scripts/aws-bootstrap.sh profile default us-east-1     # writes the `wartide` CLI profile
```

See `docs/DEPLOY.md` for the full runbook. The GitHub Actions deploy role (`wartide-prod-deploy`, created by
Terraform) is separately scoped to ECR push and ECS deploy for this one service.
