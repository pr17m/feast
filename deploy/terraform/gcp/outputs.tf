output "url" {
  value = google_cloud_run_v2_service.app.uri
}

output "image_repository" {
  value = "${var.region}-docker.pkg.dev/${var.project_id}/${google_artifact_registry_repository.images.repository_id}"
}

output "seed_password" {
  description = "Password for the seeded admin/owner accounts (admin@feast.demo …)."
  value       = random_password.seed.result
  sensitive   = true
}

output "logs" {
  value = "gcloud run services logs tail ${google_cloud_run_v2_service.app.name} --region ${var.region} --project ${var.project_id}"
}
