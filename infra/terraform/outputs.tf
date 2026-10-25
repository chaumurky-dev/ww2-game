output "alb_dns_name" {
  value = aws_lb.main.dns_name
}

output "ecr_repository_url" {
  value = aws_ecr_repository.app.repository_url
}

output "ecs_cluster" {
  value = aws_ecs_cluster.main.name
}

output "ecs_service" {
  value = aws_ecs_service.app.name
}

output "ecs_task_family" {
  value = aws_ecs_task_definition.app.family
}

output "deploy_role_arn" {
  value = aws_iam_role.deploy.arn
}

output "log_group" {
  value = aws_cloudwatch_log_group.app.name
}

output "acm_validation_records" {
  description = "DNS records to create manually when no Route53 zone is managed."
  value = [for dvo in aws_acm_certificate.main.domain_validation_options : {
    name  = dvo.resource_record_name
    type  = dvo.resource_record_type
    value = dvo.resource_record_value
  }]
}

output "ses_records" {
  description = "SES DNS records to create manually when no Route53 zone is managed."
  value = {
    verification_txt = { name = "_amazonses.${var.domain_name}", value = aws_ses_domain_identity.main.verification_token }
    dkim_cnames      = [for t in aws_ses_domain_dkim.main.dkim_tokens : { name = "${t}._domainkey.${var.domain_name}", value = "${t}.dkim.amazonses.com" }]
  }
}

output "route53_name_servers" {
  description = "Add these as NS records for the domain_name host at the parent DNS provider (empty unless create_route53_zone)."
  value       = var.create_route53_zone ? aws_route53_zone.main[0].name_servers : []
}

output "ecr_repository_name" {
  value = aws_ecr_repository.app.name
}

output "aws_region" {
  value = var.region
}
