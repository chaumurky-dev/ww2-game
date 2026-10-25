# IAM for operating Wartide

- `wartide-terraform-policy.json`: least-privilege policy for the `wartide-terraform` role that the `wartide`
  AWS CLI profile assumes to run Terraform. Allows regional (us-east-1) management of the services Terraform uses,
  Route53, Secrets Manager only for `wartide-*` secrets, IAM only for `wartide-*` roles/users/policies plus
  service-linked roles, and the state bucket. Explicitly denies changing the `wartide-terraform` role itself and
  attaching `AdministratorAccess` to anything.
- `wartide-terraform-trust.json`: trust policy template; `__ADMIN_PRINCIPAL_ARN__` is substituted by the script
  (only that principal may assume the role).

Create or update everything with the script (run as an admin profile, not via the role; idempotent, prints each
AWS command, publishes a new policy version only when the JSON changed and keeps at most 4 versions):

```sh
AWS_PROFILE=default ADMIN_PRINCIPAL_ARN=arn:aws:iam::<acct>:user/<admin> scripts/aws-bootstrap.sh operator-role
scripts/aws-bootstrap.sh profile default us-east-1     # writes the `wartide` CLI profile
```

See `docs/DEPLOY.md` for the full runbook. The GitHub Actions deploy role (`wartide-prod-deploy`, created by
Terraform) is separately scoped to ECR push and ECS deploy for this one service.
