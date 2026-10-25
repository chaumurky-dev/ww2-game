# Account guardrails and alerting. Everything notifies var.alert_email.

# ------------------------------------------------------------ SNS alerts
resource "aws_sns_topic" "alerts" {
  name = "${local.name}-alerts"
}

# The subscription stays "pending confirmation" until the owner clicks the link in the email AWS sends.
resource "aws_sns_topic_subscription" "alerts_email" {
  topic_arn = aws_sns_topic.alerts.arn
  protocol  = "email"
  endpoint  = var.alert_email
}

data "aws_iam_policy_document" "alerts" {
  statement {
    sid       = "CloudWatchAlarms"
    actions   = ["sns:Publish"]
    resources = [aws_sns_topic.alerts.arn]
    principals {
      type        = "Service"
      identifiers = ["cloudwatch.amazonaws.com"]
    }
    condition {
      test     = "StringEquals"
      variable = "aws:SourceAccount"
      values   = [data.aws_caller_identity.current.account_id]
    }
  }
  statement {
    sid       = "CostAnomalyDetection"
    actions   = ["sns:Publish"]
    resources = [aws_sns_topic.alerts.arn]
    principals {
      type        = "Service"
      identifiers = ["costalerts.amazonaws.com"]
    }
  }
  statement {
    sid       = "OwnerAccount"
    actions   = ["sns:Publish", "sns:Subscribe", "sns:GetTopicAttributes", "sns:ListSubscriptionsByTopic"]
    resources = [aws_sns_topic.alerts.arn]
    principals {
      type        = "AWS"
      identifiers = ["arn:aws:iam::${data.aws_caller_identity.current.account_id}:root"]
    }
  }
}

resource "aws_sns_topic_policy" "alerts" {
  arn    = aws_sns_topic.alerts.arn
  policy = data.aws_iam_policy_document.alerts.json
}

# ---------------------------------------------------------------- Budget
resource "aws_budgets_budget" "monthly" {
  name         = "${local.name}-monthly"
  budget_type  = "COST"
  limit_amount = tostring(var.monthly_budget_usd)
  limit_unit   = "USD"
  time_unit    = "MONTHLY"

  notification {
    comparison_operator        = "GREATER_THAN"
    threshold                  = 50
    threshold_type             = "PERCENTAGE"
    notification_type          = "ACTUAL"
    subscriber_email_addresses = [var.alert_email]
  }
  notification {
    comparison_operator        = "GREATER_THAN"
    threshold                  = 100
    threshold_type             = "PERCENTAGE"
    notification_type          = "ACTUAL"
    subscriber_email_addresses = [var.alert_email]
  }
  notification {
    comparison_operator        = "GREATER_THAN"
    threshold                  = 100
    threshold_type             = "PERCENTAGE"
    notification_type          = "FORECASTED"
    subscriber_email_addresses = [var.alert_email]
  }
}

# ------------------------------------------------------ Cost anomalies
resource "aws_ce_anomaly_monitor" "service" {
  name              = "${local.name}-services"
  monitor_type      = "DIMENSIONAL"
  monitor_dimension = "SERVICE"
}

# IMMEDIATE frequency only supports SNS subscribers (EMAIL needs DAILY/WEEKLY), so the
# subscription publishes to the alerts topic, which emails var.alert_email.
resource "aws_ce_anomaly_subscription" "service" {
  name             = "${local.name}-anomalies"
  frequency        = "IMMEDIATE"
  monitor_arn_list = [aws_ce_anomaly_monitor.service.arn]

  subscriber {
    type    = "SNS"
    address = aws_sns_topic.alerts.arn
  }

  threshold_expression {
    dimension {
      key           = "ANOMALY_TOTAL_IMPACT_ABSOLUTE"
      match_options = ["GREATER_THAN_OR_EQUAL"]
      values        = ["10"]
    }
  }

  depends_on = [aws_sns_topic_policy.alerts]
}

# ------------------------------------------------------------ GuardDuty
# If the account already has a detector in this region, import it instead:
#   terraform import aws_guardduty_detector.main <detector-id>
resource "aws_guardduty_detector" "main" {
  enable = true
}

# ------------------------------------------ Account-wide S3 public access
resource "aws_s3_account_public_access_block" "main" {
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

# ------------------------------------------------------ Account contacts
locals {
  alternate_contacts = toset(["SECURITY", "OPERATIONS", "BILLING"])
}

resource "aws_account_alternate_contact" "owner" {
  for_each               = local.alternate_contacts
  alternate_contact_type = each.value
  name                   = "Wartide owner"
  title                  = "Owner"
  email_address          = var.alert_email
  phone_number           = var.alert_phone
}

# --------------------------------------------------------------- Alarms
# Log flood / abuse: more than 200 MB ingested into the app log group in an hour.
resource "aws_cloudwatch_metric_alarm" "log_ingest" {
  alarm_name          = "${local.name}-log-incoming-bytes"
  alarm_description   = "App log group ingested more than 200 MB in one hour (log flood or abuse)."
  namespace           = "AWS/Logs"
  metric_name         = "IncomingBytes"
  dimensions          = { LogGroupName = aws_cloudwatch_log_group.app.name }
  statistic           = "Sum"
  period              = 3600
  evaluation_periods  = 1
  threshold           = 200 * 1024 * 1024
  comparison_operator = "GreaterThanThreshold"
  treat_missing_data  = "notBreaching"
  alarm_actions       = [aws_sns_topic.alerts.arn]
}

# Traffic flood: more than 50000 requests in 5 minutes.
resource "aws_cloudwatch_metric_alarm" "alb_requests" {
  alarm_name          = "${local.name}-alb-request-count"
  alarm_description   = "ALB served more than 50000 requests in 5 minutes (flood or abuse)."
  namespace           = "AWS/ApplicationELB"
  metric_name         = "RequestCount"
  dimensions          = { LoadBalancer = aws_lb.main.arn_suffix }
  statistic           = "Sum"
  period              = 300
  evaluation_periods  = 1
  threshold           = 50000
  comparison_operator = "GreaterThanThreshold"
  treat_missing_data  = "notBreaching"
  alarm_actions       = [aws_sns_topic.alerts.arn]
}
