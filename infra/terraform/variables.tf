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
  description = "RDS automated backup retention. 1 is the cheap default; use 7+ for production."
  type        = number
  default     = 1
}

variable "db_deletion_protection" {
  type    = bool
  default = false
}

variable "db_skip_final_snapshot" {
  description = "Skip the final snapshot on destroy. Set false (with db_deletion_protection = true) to harden."
  type        = bool
  default     = true
}
