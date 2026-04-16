# ADR 001: Agent Loop Architecture

**Status:** Accepted  
**Date:** 2024-01-15

## Context

The agent needs a mechanism to interact with an LLM API, execute tools based on model responses, and continue until a final response is generated. We evaluated several approaches for orchestrating this loop.

## Decision

We use an iterative agent loop with tool execution:

1. Send messages to the LLM
2. Receive response
3. If response contains tool calls, execute them and append results
4. If response is a final message, return it
5. Repeat from step 1

## Rationale

- **Simplicity**: Clear, predictable control flow
- **Debuggability**: Each step is observable
- **Extensibility**: New tools are easily added
- **Resumability**: Sessions can be persisted between steps

## Consequences

- **Positive**: Minimal abstraction overhead, easy to trace execution
- **Negative**: Higher latency due to sequential tool execution
- **Negative**: Requires careful handling of tool call limits to prevent infinite loops

## Alternatives Considered

- **Batch tool execution**: Execute all tool calls in parallel. Rejected because tool dependencies require sequential execution in many cases.
- **Streaming with immediate execution**: Stream responses and execute tools as they arrive. Rejected due to complexity in handling partial tool call data.

## References

- OpenAI Tool Use documentation
- Anthropic Model Context Protocol
