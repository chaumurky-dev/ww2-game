#!/usr/bin/env python3
"""Print a presigned S3 URL (SigV4 query auth) for one GET or PUT, signed with the current AWS CLI credentials.

Stdlib only (no boto3 on the operator Mac). Used by scripts/aws-hibernate.sh so the one-off database
backup/restore task can move the dump to and from the state bucket with a single-object URL instead of
S3 permissions of its own (the task-role permissions boundary has none).

Usage: s3-presign.py GET|PUT s3://bucket/key [EXPIRES_SECONDS]
Credentials come from the active profile (AWS_PROFILE; assumed-role profiles are assumed here); the expiry is capped
to the session's remaining lifetime because a URL signed with temporary credentials dies with them.
"""
import datetime
import hashlib
import hmac
import json
import os
import subprocess
import sys
import urllib.parse


def _hmac(key: bytes, msg: str) -> bytes:
    return hmac.new(key, msg.encode(), hashlib.sha256).digest()


def _aws(*args: str) -> str:
    return subprocess.check_output(["aws", *args], text=True, stderr=subprocess.DEVNULL).strip()


def _credentials() -> dict:
    """Credentials for the active profile as {AccessKeyId, SecretAccessKey, SessionToken?, Expiration?}.

    `aws configure export-credentials` (CLI >= 2.9) does this for any profile type; older CLIs (the operator Mac
    has 2.3) get the role assumed here from the profile's role_arn/source_profile, or static keys.
    """
    try:
        return json.loads(_aws("configure", "export-credentials", "--format", "process"))
    except subprocess.CalledProcessError:
        pass
    profile = os.environ.get("AWS_PROFILE", "default")
    try:
        role_arn = _aws("configure", "get", "role_arn", "--profile", profile)
    except subprocess.CalledProcessError:
        role_arn = ""
    if role_arn:
        source = _aws("configure", "get", "source_profile", "--profile", profile) or "default"
        out = json.loads(_aws("sts", "assume-role", "--profile", source, "--role-arn", role_arn,
                              "--role-session-name", "s3-presign", "--duration-seconds", "3600"))
        return out["Credentials"]
    creds = {"AccessKeyId": _aws("configure", "get", "aws_access_key_id", "--profile", profile),
             "SecretAccessKey": _aws("configure", "get", "aws_secret_access_key", "--profile", profile)}
    try:
        creds["SessionToken"] = _aws("configure", "get", "aws_session_token", "--profile", profile)
    except subprocess.CalledProcessError:
        pass
    return creds


def main() -> int:
    if len(sys.argv) not in (3, 4) or sys.argv[1].upper() not in ("GET", "PUT") or not sys.argv[2].startswith("s3://"):
        print(__doc__, file=sys.stderr)
        return 2
    method = sys.argv[1].upper()
    bucket, _, key = sys.argv[2][5:].partition("/")
    expires = int(sys.argv[3]) if len(sys.argv) == 4 else 3600
    if not bucket or not key:
        print("error: need s3://bucket/key", file=sys.stderr)
        return 2
    region = os.environ.get("AWS_REGION") or os.environ.get("AWS_DEFAULT_REGION") or "us-east-1"

    creds = _credentials()
    akid, secret, token = creds["AccessKeyId"], creds["SecretAccessKey"], creds.get("SessionToken")
    now = datetime.datetime.now(datetime.timezone.utc)
    if creds.get("Expiration"):
        exp = datetime.datetime.fromisoformat(creds["Expiration"].replace("Z", "+00:00"))
        remaining = int((exp - now).total_seconds()) - 60
        if remaining < 120:
            print("error: AWS session credentials expire in under 3 minutes; run any aws command to refresh them and retry", file=sys.stderr)
            return 1
        expires = min(expires, remaining)

    amz_date = now.strftime("%Y%m%dT%H%M%SZ")
    date = now.strftime("%Y%m%d")
    host = f"{bucket}.s3.{region}.amazonaws.com"
    scope = f"{date}/{region}/s3/aws4_request"
    query = {
        "X-Amz-Algorithm": "AWS4-HMAC-SHA256",
        "X-Amz-Credential": f"{akid}/{scope}",
        "X-Amz-Date": amz_date,
        "X-Amz-Expires": str(expires),
        "X-Amz-SignedHeaders": "host",
    }
    if token:
        query["X-Amz-Security-Token"] = token
    quote = lambda s: urllib.parse.quote(s, safe="-_.~")  # noqa: E731
    canonical_query = "&".join(f"{quote(k)}={quote(v)}" for k, v in sorted(query.items()))
    canonical_uri = urllib.parse.quote(f"/{key}", safe="/-_.~")
    canonical_request = "\n".join([method, canonical_uri, canonical_query, f"host:{host}\n", "host", "UNSIGNED-PAYLOAD"])
    string_to_sign = "\n".join(["AWS4-HMAC-SHA256", amz_date, scope, hashlib.sha256(canonical_request.encode()).hexdigest()])
    signing_key = _hmac(_hmac(_hmac(_hmac(("AWS4" + secret).encode(), date), region), "s3"), "aws4_request")
    signature = hmac.new(signing_key, string_to_sign.encode(), hashlib.sha256).hexdigest()
    print(f"https://{host}{canonical_uri}?{canonical_query}&X-Amz-Signature={signature}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
