output "url" {
  description = "Open this. (With domain_name set, point that domain at alb_dns_name.)"
  value       = local.url
}

output "alb_dns_name" {
  value = aws_lb.app.dns_name
}

output "api_repository" {
  value = aws_ecr_repository.api.repository_url
}

output "web_repository" {
  value = aws_ecr_repository.web.repository_url
}

output "registry" {
  value = split("/", aws_ecr_repository.api.repository_url)[0]
}

output "seed_password" {
  description = "Password for the seeded admin/owner accounts (admin@feast.demo …)."
  value       = random_password.seed.result
  sensitive   = true
}

output "logs" {
  value = "aws logs tail ${aws_cloudwatch_log_group.app.name} --follow --region ${var.region}"
}

output "region" {
  value = var.region
}

output "platform" {
  description = "docker buildx --platform for this stack's images."
  value       = var.cpu_architecture == "ARM64" ? "linux/arm64" : "linux/amd64"
}
