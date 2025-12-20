# Populate

This is a migration script that uploads all thumbnails from a local directory to an S3 bucket and sets the corresponding Redis keys. Env variables are on the format:

```
THUMBNAILS_DIR=path/to/thumbnails

S3_ENDPOINT=http://localhost:9000
S3_REGION=unknown
S3_PATH_STYLE=true # For local development
S3_BUCKET=thumbs
S3_ACCESS_KEY=admin
S3_SECRET_KEY=password123

REDIS_URL=redis://localhost:6379
```
