export type Job = {
  id: string;
  name: string;
  description?: string;
  tags?: string[];
  tags_json?: string;
  enabled: boolean;
  definition_json: string;
};

export type Schedule =
  | { kind: "cron"; expression: string; timezone: { mode: "local" | "iana"; name?: string } }
  | { kind: "every"; interval: number; anchor: number }
  | { kind: "at"; at: number };

export type SecretValue =
  | { source: "inline"; value: string }
  | { source: "environment"; value: string };

export type HttpTarget = {
  kind: "http";
  method: string;
  url: string;
  success_statuses: number[];
  follow_redirects: boolean;
  headers: Record<string, SecretValue>;
  body: number[] | "<redacted>" | null;
  body_file?: string;
};

export type Target =
  | { kind: "process"; executable: string; args: string[] }
  | { kind: "shell"; command: string; shell: string }
  | HttpTarget;

export type Definition = {
  schedule: Schedule;
  target: Target;
  cwd: string;
  environment: { file?: string; path?: string; values: Record<string, string> };
  policy: {
    overlap: "skip" | "replace" | "allow";
    missed_run: "skip" | "latest" | "all";
    start_deadline: number | null;
    catch_up_limit: number;
    retries: number;
    retry_delay: number;
    retry_cap: number;
    backoff: "fixed" | "exponential";
    retry_timeout: boolean;
    timeout: number | null;
    termination_grace: number;
    per_job_concurrency: number;
  };
};

export type Run = {
  id: string;
  job_id: string;
  job_name?: string;
  requested_at_us: number;
  trigger: string;
  state: string;
  duration_us?: number;
};

export type RunDetailData = Run & {
  attempts: Array<{ attempt_number: number; state: string; duration_us?: number; error?: string }>;
  snapshot_json?: string;
};
