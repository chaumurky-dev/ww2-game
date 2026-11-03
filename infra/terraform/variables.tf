variable "env" {
  description = "Environment name, used as a suffix on resource names."
  type        = string
  default     = "prod"
}

variable "region" {
  description = "AWS region. SES must be available here."
  type        = string
  default     = "us-east-1"
}

variable "domain_name" {
  description = "Public FQDN of the app (e.g. wartide.example.com). Used for ACM, SES and BASE_URL."
  type        = string
}

variable "route53_zone_id" {
  description = "Existing Route53 hosted zone ID for domain_name (ignored if create_route53_zone). Leave empty to create DNS records manually."
  type        = string
  default     = ""
}

variable "create_route53_zone" {
  description = "Create a Route53 hosted zone whose apex is domain_name (delegate it from the parent zone via NS records). When true, route53_zone_id is ignored."
  type        = bool
  default     = false
}

variable "github_repo" {
  description = "GitHub repository allowed to assume the deploy role, as owner/repo."
  type        = string
}

variable "github_oidc_subjects" {
  description = <<-EOT
    OIDC subject claims allowed to assume the deploy role. Empty = "repo:<github_repo>:ref:refs/heads/main".
    Organizations with GitHub's "immutable subject" setting issue subjects like
    "repo:<owner>@<owner_id>/<repo>@<repo_id>:ref:refs/heads/main"; look it up with
    `gh api repos/<owner>/<repo>/actions/oidc/customization/sub` (sub_claim_prefix).
  EOT
  type        = list(string)
  default     = []
}

variable "create_github_oidc_provider" {
  description = "Create the account-wide GitHub OIDC provider. Set false if it already exists."
  type        = bool
  default     = true
}

variable "vpc_cidr" {
  type    = string
  default = "10.20.0.0/16"
}

variable "use_fargate_spot" {
  description = "Run the service on FARGATE_SPOT (cheaper, may be interrupted) instead of on-demand FARGATE."
  type        = bool
  default     = true
}

variable "task_cpu" {
  type    = number
  default = 256
}

variable "task_memory" {
  type    = number
  default = 512
}

variable "desired_count" {
  type    = number
  default = 1
}

variable "enable_autoscaling" {
  type    = bool
  default = false
}

variable "max_count" {
  type    = number
  default = 3
}

variable "db_instance_class" {
  type    = string
  default = "db.t4g.micro"
}

variable "db_allocated_storage" {
  type    = number
  default = 20
}

variable "db_multi_az" {
  type    = bool
  default = false
}

variable "ses_from_local_part" {
  description = "Local part of the From address (noreply@domain_name)."
  type        = string
  default     = "noreply"
}

variable "container_image" {
  description = "Initial image for the task definition. CI deploys replace it."
  type        = string
  default     = ""
}

variable "log_retention_days" {
  type    = number
  default = 7
}

variable "db_backup_retention_days" {
  description = "RDS automated backup retention in days."
  type        = number
  default     = 7
}

variable "db_deletion_protection" {
  description = "RDS deletion protection. Set false (and apply) before a destroy."
  type        = bool
  default     = true
}

variable "db_skip_final_snapshot" {
  description = "Skip the final RDS snapshot on destroy. Keep false in production. scripts/aws-hibernate.sh sets it true for the destroying apply because a trial-restored pg_dump is already verified in S3 (docs/DEPLOY.md section 7)."
  type        = bool
  default     = false
}

variable "alert_email" {
  description = "Email for budget, cost-anomaly, CloudWatch alarm and account (security/billing/operations) notifications. Required."
  type        = string
}

variable "anomaly_monitor_arn" {
  description = "ARN of an existing Cost Explorer anomaly monitor to subscribe to. AWS allows one DIMENSIONAL (service) monitor per account and creates \"Default-Services-Monitor\" itself in many accounts; set this to its ARN (aws ce get-anomaly-monitors) instead of creating a second one. null = create our own."
  type        = string
  default     = null
}

variable "alert_phone" {
  description = "Phone number (E.164, e.g. +14155550123) for the account alternate contacts. The default is a placeholder: replace it."
  type        = string
  default     = "+10000000000"
}

variable "monthly_budget_usd" {
  description = "Monthly cost budget in USD; alerts at 50% and 100% actual and 100% forecast."
  type        = number
  default     = 60
}

variable "hibernate" {
  description = "Tear down everything with an hourly or fixed monthly price (ECS service and autoscaling, ALB, RDS, Route53 zone and all its records, SES identity and DKIM, the four secrets, both task definitions, both alarms, GuardDuty detector); keep the free resources (VPC, security groups, ECS cluster, ECR repository, IAM/OIDC, ACM certificate, S3 buckets, SNS, budget, anomaly subscription, log group, account settings). The data is kept as a pg_dump in the state bucket by scripts/aws-hibernate.sh."
  type        = bool
  default     = false
}

variable "alb_deletion_protection" {
  description = "ALB deletion protection. Set false (and apply) before hibernating."
  type        = bool
  default     = true
}

variable "db_final_snapshot_tag" {
  description = "Suffix of the final RDS snapshot name (<name>-final-<tag>); scripts/aws-hibernate.sh sets it to the date."
  type        = string
  default     = "manual"
}

variable "db_restore_snapshot" {
  description = "RDS snapshot identifier to restore from when waking (see scripts/aws-hibernate.sh)."
  type        = string
  default     = null
}

variable "secret_recovery_window_days" {
  description = "Secrets Manager recovery window for the prod secrets. scripts/aws-hibernate.sh sets it to 0 in the apply before hibernating so the names are free immediately on wake (destroy uses the value recorded in state)."
  type        = number
  default     = 7
}

variable "route53_delegation_set_id" {
  description = "Id (just the id, e.g. N1PA6795SAMPLE, not the /delegationset/ prefix) of a reusable delegation set so the hosted zone keeps the same name servers when it is destroyed and recreated. Created once by the account owner with scripts/aws-bootstrap.sh delegation-set. Changing it replaces the zone."
  type        = string
  default     = null
}
