Read the following schema for jsonl items. I want to know if these are supported with `json-stream`. If so, are they written to the session files. If they aren't, should they be.

```
  // Emitted as the last message on success
  | {
      type: "result";
      subtype: "success";
      duration_ms: number;
      is_error: false;
      num_turns: number;
      result: string;
      session_id: string;
      usage?: {
        input_tokens: number;
		max_tokens: number;
        cache_creation_input_tokens?: number;
        cache_read_input_tokens?: number;
        output_tokens: number;
        service_tier?: string;
      };
      permission_denials?: string[];
    }

  // Emitted as the last message on error
  | {
      type: "result";
      subtype: "error_during_execution" | "error_max_turns";
      duration_ms: number;
      is_error: true;
      num_turns: number;
      error: string;
      session_id: string;
      usage?: {
        input_tokens: number;
		max_tokens: number;
        cache_creation_input_tokens?: number;
        cache_read_input_tokens?: number;
        output_tokens: number;
        service_tier?: string;
      };
      permission_denials?: string[];
    } 
```
