output "alb_dns_name" {
  value = one(aws_lb.main[*].dns_name)
}

output "ecr_repository_url" {
  value = aws_ecr_repository.app.repository_url
}

output "ecs_cluster" {
  value = aws_ecs_cluster.main.name
}

output "ecs_service" {
  value = one(aws_ecs_service.app[*].name)
}

output "ecs_task_family" {
  value = one(aws_ecs_task_definition.app[*].family)
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
  value = try({
    verification_txt = { name = "_amazonses.${var.domain_name}", value = aws_ses_domain_identity.main[0].verification_token }
    dkim_cnames      = [for t in aws_ses_domain_dkim.main[0].dkim_tokens : { name = "${t}._domainkey.${var.domain_name}", value = "${t}.dkim.amazonses.com" }]
  }, null)
}

output "route53_name_servers" {
  description = "Add these as NS records for the domain_name host at the parent DNS provider (empty unless create_route53_zone)."
  value       = try(aws_route53_zone.main[0].name_servers, [])
}

output "ecr_repository_name" {
  value = aws_ecr_repository.app.name
}

output "aws_region" {
  value = var.region
}

output "route53_zone_id" {
  description = "Hosted zone id (empty while hibernated)."
  value       = try(aws_route53_zone.main[0].zone_id, var.route53_zone_id)
}

output "public_subnet_ids" {
  description = "Public subnets (ECS tasks run here); used by scripts/aws-hibernate.sh for the dbtool task."
  value       = aws_subnet.public[*].id
}

output "ecs_security_group_id" {
  description = "Security group of the ECS tasks; used by scripts/aws-hibernate.sh for the dbtool task."
  value       = aws_security_group.ecs.id
}

output "dbtool_task_family" {
  description = "Family of the one-off database dump/restore task (null while hibernated)."
  value       = one(aws_ecs_task_definition.dbtool[*].family)
}

output "domain_name" {
  value = var.domain_name
}
