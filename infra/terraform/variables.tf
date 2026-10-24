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
  description = "Route53 hosted zone ID for domain_name. Leave empty to create DNS records manually."
  type        = string
  default     = ""
}

variable "github_repo" {
  description = "GitHub repository allowed to assume the deploy role, as owner/repo."
  type        = string
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

variable "task_cpu" {
  type    = number
  default = 512
}

variable "task_memory" {
  type    = number
  default = 1024
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
  default = 30
}
