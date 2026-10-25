# IAM for operating Wartide

`wartide-terraform-policy.json` is the least-privilege policy for the `wartide-terraform` role that the
`wartide` AWS CLI profile assumes to run Terraform. It allows: regional (us-east-1) management of the
infrastructure services Terraform uses, Route53 zone/record changes, Secrets Manager only for
`wartide-*` secrets, IAM only for `wartide-*` roles/users/policies plus the service-linked roles AWS
creates on first use, and the Terraform state bucket. It explicitly denies changing the
`wartide-terraform` role itself and attaching `AdministratorAccess` to anything.

Apply or update it (as an admin, not via the role):
```sh
aws iam create-policy --policy-name wartide-terraform \
  --policy-document file://infra/iam/wartide-terraform-policy.json        # first time
aws iam create-policy-version --policy-arn arn:aws:iam::243963726301:policy/wartide-terraform \
  --policy-document file://infra/iam/wartide-terraform-policy.json --set-as-default   # updates
aws iam attach-role-policy --role-name wartide-terraform --policy-arn arn:aws:iam::243963726301:policy/wartide-terraform
aws iam detach-role-policy --role-name wartide-terraform --policy-arn arn:aws:iam::aws:policy/AdministratorAccess
```
The GitHub Actions deploy role (`wartide-prod-deploy`, created by Terraform) is separately scoped to
ECR push and ECS deploy for this one service.
