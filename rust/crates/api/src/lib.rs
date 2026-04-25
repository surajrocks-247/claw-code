mod client;
mod error;
mod http_client;
mod prompt_cache;
mod providers;
mod sse;
mod types;

pub use client::{
    oauth_token_is_expired, read_base_url, read_xai_base_url, resolve_saved_oauth_token,
    resolve_startup_auth_source, MessageStream, OAuthTokenSet, ProviderClient,
};
pub use error::ApiError;
pub use http_client::{
    build_http_client, build_http_client_or_default, build_http_client_with, ProxyConfig,
};
pub use prompt_cache::{
    CacheBreakEvent, PromptCache, PromptCacheConfig, PromptCachePaths, PromptCacheRecord,
    PromptCacheStats,
};
pub use providers::anthropic::{AnthropicClient, AnthropicClient as ApiClient, AuthSource};
pub use providers::openai_compat::{
    build_chat_completion_request, flatten_tool_result_content, is_reasoning_model,
    model_rejects_is_error_field, translate_message, OpenAiCompatClient, OpenAiCompatConfig,
};
pub use providers::{
    detect_provider_kind, max_tokens_for_model, max_tokens_for_model_with_override,
    resolve_model_alias, ProviderKind,
};
pub use sse::{parse_frame, SseParser};
pub use types::{
    ContentBlockDelta, ContentBlockDeltaEvent, ContentBlockStartEvent, ContentBlockStopEvent,
    InputContentBlock, InputMessage, MessageDelta, MessageDeltaEvent, MessageRequest,
    MessageResponse, MessageStartEvent, MessageStopEvent, OutputContentBlock, StreamEvent,
    ToolChoice, ToolDefinition, ToolResultContentBlock, Usage,
};

pub use telemetry::{
    AnalyticsEvent, AnthropicRequestProfile, ClientIdentity, JsonlTelemetrySink,
    MemoryTelemetrySink, SessionTraceRecord, SessionTracer, TelemetryEvent, TelemetrySink,
    DEFAULT_ANTHROPIC_VERSION,
};

#[cfg(test)]
mod tests {
    //! Tests for the `api` crate root (`lib.rs`).
    //!
    //! The crate root is a re-export hub: it gathers types, traits, and
    //! functions from internal modules and re-exports them as the public API.
    //! These tests verify that:
    //!
    //! 1. Every re-exported symbol is actually reachable through `crate::`.
    //! 2. Key types can be constructed, serialized, and deserialized.
    //! 3. Builder / convenience constructors produce the expected defaults.
    //! 4. Cross-module types compose correctly (e.g. `MessageRequest` with
    //!    `ToolDefinition`, `Usage` with `MessageResponse`, etc.).

    use serde_json::json;

    // -----------------------------------------------------------------------
    // Re-export accessibility: importing via crate root must compile.
    // -----------------------------------------------------------------------

    use crate::{
        // client.rs re-exports
        ProviderClient,
        // error.rs
        ApiError,
        // http_client.rs
        build_http_client_with, ProxyConfig,
        // prompt_cache.rs
        CacheBreakEvent, PromptCacheConfig, PromptCachePaths, PromptCacheRecord,
        PromptCacheStats,
        // providers/mod.rs
        detect_provider_kind, max_tokens_for_model, max_tokens_for_model_with_override,
        resolve_model_alias, ProviderKind,
        // providers/openai_compat.rs
        build_chat_completion_request, flatten_tool_result_content, is_reasoning_model,
        model_rejects_is_error_field, translate_message, OpenAiCompatConfig,
        // sse.rs
        parse_frame, SseParser,
        // types.rs
        ContentBlockDelta, ContentBlockDeltaEvent, ContentBlockStopEvent, InputContentBlock,
        InputMessage, MessageRequest, MessageResponse, MessageStopEvent, OutputContentBlock,
        StreamEvent, ToolChoice, ToolDefinition, ToolResultContentBlock, Usage,
        // telemetry re-exports
        AnalyticsEvent, AnthropicRequestProfile, ClientIdentity, MemoryTelemetrySink,
        TelemetryEvent, TelemetrySink, DEFAULT_ANTHROPIC_VERSION,
    };

    // -----------------------------------------------------------------------
    // MessageRequest construction and serialization
    // -----------------------------------------------------------------------

    #[test]
    fn message_request_default_is_empty_and_serializable() {
        let request = MessageRequest::default();
        assert!(request.model.is_empty());
        assert_eq!(request.max_tokens, 0);
        assert!(request.messages.is_empty());
        assert!(request.system.is_none());
        assert!(request.tools.is_none());
        assert!(request.tool_choice.is_none());
        assert!(!request.stream);
        assert!(request.temperature.is_none());
        assert!(request.reasoning_effort.is_none());

        let json = serde_json::to_value(&request).expect("MessageRequest should serialize");
        assert!(json.is_object());
        // stream:false is skipped by `skip_serializing_if = "Not::not"`
        assert!(json.get("stream").is_none());
    }

    #[test]
    fn message_request_with_streaming_enables_stream_flag() {
        let request = MessageRequest {
            model: "claude-sonnet-4-6".to_string(),
            max_tokens: 4096,
            messages: vec![InputMessage::user_text("hello")],
            ..Default::default()
        }
        .with_streaming();

        assert!(request.stream);
        let json = serde_json::to_value(&request).expect("should serialize");
        assert_eq!(json["stream"], json!(true));
    }

    #[test]
    fn message_request_round_trips_through_serde() {
        let original = MessageRequest {
            model: "claude-opus-4-6".to_string(),
            max_tokens: 32_000,
            messages: vec![
                InputMessage::user_text("What is Rust?"),
                InputMessage {
                    role: "assistant".to_string(),
                    content: vec![InputContentBlock::Text {
                        text: "Rust is a systems programming language.".to_string(),
                    }],
                },
            ],
            system: Some("Be concise.".to_string()),
            tools: Some(vec![ToolDefinition {
                name: "search".to_string(),
                description: Some("Web search".to_string()),
                input_schema: json!({"type": "object", "properties": {"q": {"type": "string"}}}),
            }]),
            tool_choice: Some(ToolChoice::Auto),
            stream: true,
            temperature: Some(0.5),
            top_p: Some(0.9),
            frequency_penalty: Some(0.1),
            presence_penalty: Some(0.2),
            stop: Some(vec!["END".to_string()]),
            reasoning_effort: None,
        };

        let json_bytes = serde_json::to_vec(&original).expect("serialize");
        let restored: MessageRequest = serde_json::from_slice(&json_bytes).expect("deserialize");

        assert_eq!(original, restored);
    }

    // -----------------------------------------------------------------------
    // InputMessage convenience constructors
    // -----------------------------------------------------------------------

    #[test]
    fn input_message_user_text_sets_role_and_single_text_block() {
        let msg = InputMessage::user_text("hi");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content.len(), 1);
        match &msg.content[0] {
            InputContentBlock::Text { text } => assert_eq!(text, "hi"),
            other => panic!("expected Text block, got {other:?}"),
        }
    }

    #[test]
    fn input_message_user_tool_result_sets_role_and_tool_result_block() {
        let msg = InputMessage::user_tool_result("tool_42", "result data", true);
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content.len(), 1);
        match &msg.content[0] {
            InputContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                assert_eq!(tool_use_id, "tool_42");
                assert!(*is_error);
                assert_eq!(content.len(), 1);
                match &content[0] {
                    ToolResultContentBlock::Text { text } => assert_eq!(text, "result data"),
                    other => panic!("expected Text result block, got {other:?}"),
                }
            }
            other => panic!("expected ToolResult block, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // InputContentBlock serde tagging
    // -----------------------------------------------------------------------

    #[test]
    fn input_content_block_serializes_with_type_tag() {
        let text_block = InputContentBlock::Text {
            text: "hello".to_string(),
        };
        let json = serde_json::to_value(&text_block).expect("serialize");
        assert_eq!(json["type"], "text");
        assert_eq!(json["text"], "hello");

        let tool_use = InputContentBlock::ToolUse {
            id: "tu_1".to_string(),
            name: "read_file".to_string(),
            input: json!({"path": "/tmp"}),
        };
        let json = serde_json::to_value(&tool_use).expect("serialize");
        assert_eq!(json["type"], "tool_use");
        assert_eq!(json["name"], "read_file");

        let tool_result = InputContentBlock::ToolResult {
            tool_use_id: "tu_1".to_string(),
            content: vec![ToolResultContentBlock::Text {
                text: "ok".to_string(),
            }],
            is_error: false,
        };
        let json = serde_json::to_value(&tool_result).expect("serialize");
        assert_eq!(json["type"], "tool_result");
        // is_error:false should be skipped
        assert!(json.get("is_error").is_none());
    }

    // -----------------------------------------------------------------------
    // ToolChoice serde tagging
    // -----------------------------------------------------------------------

    #[test]
    fn tool_choice_variants_serialize_with_correct_tags() {
        assert_eq!(
            serde_json::to_value(ToolChoice::Auto).unwrap(),
            json!({"type": "auto"})
        );
        assert_eq!(
            serde_json::to_value(ToolChoice::Any).unwrap(),
            json!({"type": "any"})
        );
        assert_eq!(
            serde_json::to_value(ToolChoice::Tool {
                name: "bash".to_string()
            })
            .unwrap(),
            json!({"type": "tool", "name": "bash"})
        );
    }

    // -----------------------------------------------------------------------
    // OutputContentBlock / StreamEvent serde tagging
    // -----------------------------------------------------------------------

    #[test]
    fn output_content_block_text_round_trips() {
        let block = OutputContentBlock::Text {
            text: "hello world".to_string(),
        };
        let json = serde_json::to_value(&block).expect("serialize");
        assert_eq!(json["type"], "text");
        let restored: OutputContentBlock = serde_json::from_value(json).expect("deserialize");
        assert_eq!(block, restored);
    }

    #[test]
    fn output_content_block_thinking_round_trips() {
        let block = OutputContentBlock::Thinking {
            thinking: "step 1: analyze".to_string(),
            signature: Some("sig_abc".to_string()),
        };
        let json = serde_json::to_value(&block).expect("serialize");
        assert_eq!(json["type"], "thinking");
        assert_eq!(json["signature"], "sig_abc");
        let restored: OutputContentBlock = serde_json::from_value(json).expect("deserialize");
        assert_eq!(block, restored);
    }

    // -----------------------------------------------------------------------
    // Usage and MessageResponse
    // -----------------------------------------------------------------------

    #[test]
    fn usage_default_is_all_zeros() {
        let usage = Usage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.cache_creation_input_tokens, 0);
        assert_eq!(usage.cache_read_input_tokens, 0);
        assert_eq!(usage.total_tokens(), 0);
    }

    #[test]
    fn usage_total_tokens_sums_all_fields() {
        let usage = Usage {
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_input_tokens: 20,
            cache_read_input_tokens: 30,
        };
        assert_eq!(usage.total_tokens(), 200);
    }

    #[test]
    fn message_response_total_tokens_delegates_to_usage() {
        let response = MessageResponse {
            id: "msg_1".to_string(),
            kind: "message".to_string(),
            role: "assistant".to_string(),
            content: vec![OutputContentBlock::Text {
                text: "hi".to_string(),
            }],
            model: "claude-sonnet-4-6".to_string(),
            stop_reason: Some("end_turn".to_string()),
            stop_sequence: None,
            usage: Usage {
                input_tokens: 10,
                output_tokens: 5,
                cache_creation_input_tokens: 2,
                cache_read_input_tokens: 3,
            },
            request_id: Some("req_abc".to_string()),
        };
        assert_eq!(response.total_tokens(), 20);
    }

    // -----------------------------------------------------------------------
    // ContentBlockDelta variants
    // -----------------------------------------------------------------------

    #[test]
    fn content_block_delta_variants_serialize_with_correct_type_tags() {
        let text = ContentBlockDelta::TextDelta {
            text: "hi".to_string(),
        };
        let json = serde_json::to_value(&text).unwrap();
        assert_eq!(json["type"], "text_delta");

        let json_delta = ContentBlockDelta::InputJsonDelta {
            partial_json: r#"{"key"#.to_string(),
        };
        let json_val = serde_json::to_value(&json_delta).unwrap();
        assert_eq!(json_val["type"], "input_json_delta");

        let thinking = ContentBlockDelta::ThinkingDelta {
            thinking: "step 1".to_string(),
        };
        let json_val = serde_json::to_value(&thinking).unwrap();
        assert_eq!(json_val["type"], "thinking_delta");

        let sig = ContentBlockDelta::SignatureDelta {
            signature: "sig_123".to_string(),
        };
        let json_val = serde_json::to_value(&sig).unwrap();
        assert_eq!(json_val["type"], "signature_delta");
    }

    // -----------------------------------------------------------------------
    // StreamEvent — full envelope round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn stream_event_message_stop_round_trips() {
        let event = StreamEvent::MessageStop(MessageStopEvent {});
        let json = serde_json::to_value(&event).expect("serialize");
        assert_eq!(json["type"], "message_stop");
        let restored: StreamEvent = serde_json::from_value(json).expect("deserialize");
        assert_eq!(event, restored);
    }

    #[test]
    fn stream_event_content_block_stop_round_trips() {
        let event = StreamEvent::ContentBlockStop(ContentBlockStopEvent { index: 3 });
        let json = serde_json::to_value(&event).expect("serialize");
        assert_eq!(json["type"], "content_block_stop");
        assert_eq!(json["index"], 3);
        let restored: StreamEvent = serde_json::from_value(json).expect("deserialize");
        assert_eq!(event, restored);
    }

    // -----------------------------------------------------------------------
    // ApiError construction helpers
    // -----------------------------------------------------------------------

    #[test]
    fn api_error_missing_credentials_is_not_retryable_and_classifies_as_provider_auth() {
        let error = ApiError::missing_credentials("TestProvider", &["TEST_API_KEY"]);
        assert!(!error.is_retryable());
        assert_eq!(error.safe_failure_class(), "provider_auth");
        assert_eq!(error.request_id(), None);
        let rendered = error.to_string();
        assert!(rendered.contains("TestProvider"));
        assert!(rendered.contains("TEST_API_KEY"));
    }

    #[test]
    fn api_error_context_window_exceeded_classifies_correctly() {
        let error = ApiError::ContextWindowExceeded {
            model: "claude-sonnet-4-6".to_string(),
            estimated_input_tokens: 180_000,
            requested_output_tokens: 64_000,
            estimated_total_tokens: 244_000,
            context_window_tokens: 200_000,
        };
        assert!(!error.is_retryable());
        assert!(error.is_context_window_failure());
        assert_eq!(error.safe_failure_class(), "context_window");
        let rendered = error.to_string();
        assert!(rendered.contains("context_window_blocked"));
        assert!(rendered.contains("244000"));
        assert!(rendered.contains("200000"));
    }

    #[test]
    fn api_error_request_body_size_exceeded_classifies_as_request_size() {
        let error = ApiError::RequestBodySizeExceeded {
            estimated_bytes: 7_000_000,
            max_bytes: 6_291_456,
            provider: "DashScope",
        };
        assert!(!error.is_retryable());
        assert_eq!(error.safe_failure_class(), "request_size");
        let rendered = error.to_string();
        assert!(rendered.contains("7000000"));
        assert!(rendered.contains("6291456"));
        assert!(rendered.contains("DashScope"));
    }

    #[test]
    fn api_error_backoff_overflow_classifies_as_provider_transport() {
        let error = ApiError::BackoffOverflow {
            attempt: 99,
            base_delay: std::time::Duration::from_secs(1),
        };
        assert!(!error.is_retryable());
        assert_eq!(error.safe_failure_class(), "provider_transport");
        assert!(error.to_string().contains("99"));
    }

    #[test]
    fn api_error_expired_oauth_classifies_as_provider_auth() {
        let error = ApiError::ExpiredOAuthToken;
        assert!(!error.is_retryable());
        assert_eq!(error.safe_failure_class(), "provider_auth");
        assert!(error.to_string().contains("expired"));
    }

    // -----------------------------------------------------------------------
    // ProxyConfig
    // -----------------------------------------------------------------------

    #[test]
    fn proxy_config_default_is_empty() {
        let config = ProxyConfig::default();
        assert!(config.is_empty());
        assert!(config.http_proxy.is_none());
        assert!(config.https_proxy.is_none());
        assert!(config.no_proxy.is_none());
        assert!(config.proxy_url.is_none());
    }

    #[test]
    fn proxy_config_from_proxy_url_sets_unified_and_is_not_empty() {
        let config = ProxyConfig::from_proxy_url("http://proxy:8080");
        assert!(!config.is_empty());
        assert_eq!(config.proxy_url.as_deref(), Some("http://proxy:8080"));
        assert!(config.http_proxy.is_none());
        assert!(config.https_proxy.is_none());
    }

    #[test]
    fn build_http_client_with_default_config_succeeds() {
        let client = build_http_client_with(&ProxyConfig::default());
        assert!(client.is_ok());
    }

    // -----------------------------------------------------------------------
    // SSE parsing via crate-root re-export
    // -----------------------------------------------------------------------

    #[test]
    fn parse_frame_returns_none_for_empty_or_ping() {
        assert_eq!(parse_frame("").unwrap(), None);
        assert_eq!(parse_frame("event: ping\n\n").unwrap(), None);
        assert_eq!(parse_frame(": keepalive comment\n\n").unwrap(), None);
        assert_eq!(parse_frame("data: [DONE]\n\n").unwrap(), None);
    }

    #[test]
    fn sse_parser_buffers_partial_frames_and_emits_on_completion() {
        let mut parser = SseParser::new();

        // First push: incomplete frame
        let events = parser
            .push(b"event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hel")
            .expect("push should buffer");
        assert!(events.is_empty(), "incomplete frame should not emit events");

        // Second push: complete the frame
        let events = parser
            .push(b"lo\"}}\n\n")
            .expect("completing the frame should parse");
        assert_eq!(events.len(), 1);
        match &events[0] {
            StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                index,
                delta: ContentBlockDelta::TextDelta { text },
            }) => {
                assert_eq!(*index, 0);
                assert_eq!(text, "hello");
            }
            other => panic!("expected text delta, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Provider / model resolution via crate-root re-exports
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_model_alias_maps_known_aliases() {
        assert_eq!(resolve_model_alias("opus"), "claude-opus-4-6");
        assert_eq!(resolve_model_alias("sonnet"), "claude-sonnet-4-6");
        assert_eq!(resolve_model_alias("grok"), "grok-3");
        assert_eq!(resolve_model_alias("grok-mini"), "grok-3-mini");
        assert_eq!(resolve_model_alias("kimi"), "kimi-k2.5");
    }

    #[test]
    fn resolve_model_alias_passes_through_unknown_models() {
        assert_eq!(resolve_model_alias("my-custom-model"), "my-custom-model");
        assert_eq!(
            resolve_model_alias("claude-sonnet-4-6"),
            "claude-sonnet-4-6"
        );
    }

    #[test]
    fn resolve_model_alias_is_case_insensitive_for_aliases() {
        assert_eq!(resolve_model_alias("OPUS"), "claude-opus-4-6");
        assert_eq!(resolve_model_alias("Grok"), "grok-3");
        assert_eq!(resolve_model_alias("KIMI"), "kimi-k2.5");
    }

    #[test]
    fn detect_provider_kind_uses_model_name_prefix() {
        assert_eq!(
            detect_provider_kind("claude-sonnet-4-6"),
            ProviderKind::Anthropic
        );
        assert_eq!(detect_provider_kind("grok-3"), ProviderKind::Xai);
        assert_eq!(detect_provider_kind("openai/gpt-4o"), ProviderKind::OpenAi);
        assert_eq!(detect_provider_kind("qwen-plus"), ProviderKind::OpenAi);
    }

    #[test]
    fn max_tokens_for_model_returns_expected_limits() {
        assert_eq!(max_tokens_for_model("claude-opus-4-6"), 32_000);
        assert_eq!(max_tokens_for_model("claude-sonnet-4-6"), 64_000);
        assert_eq!(max_tokens_for_model("grok-3"), 64_000);
        assert_eq!(max_tokens_for_model("kimi-k2.5"), 16_384);
    }

    #[test]
    fn max_tokens_for_model_with_override_prefers_override() {
        assert_eq!(
            max_tokens_for_model_with_override("claude-opus-4-6", Some(8_000)),
            8_000
        );
        assert_eq!(
            max_tokens_for_model_with_override("claude-opus-4-6", None),
            32_000
        );
    }

    // -----------------------------------------------------------------------
    // OpenAI-compat helpers via crate-root re-exports
    // -----------------------------------------------------------------------

    #[test]
    fn is_reasoning_model_identifies_reasoning_families() {
        assert!(is_reasoning_model("o1-mini"));
        assert!(is_reasoning_model("o3-mini"));
        assert!(is_reasoning_model("o4-mini"));
        assert!(is_reasoning_model("grok-3-mini"));
        assert!(is_reasoning_model("qwen-qwq-32b"));
        assert!(!is_reasoning_model("gpt-4o"));
        assert!(!is_reasoning_model("claude-sonnet-4-6"));
        assert!(!is_reasoning_model("grok-3"));
    }

    #[test]
    fn model_rejects_is_error_field_identifies_kimi_models() {
        assert!(model_rejects_is_error_field("kimi-k2.5"));
        assert!(model_rejects_is_error_field("kimi-k1.5"));
        assert!(!model_rejects_is_error_field("gpt-4o"));
        assert!(!model_rejects_is_error_field("grok-3"));
    }

    #[test]
    fn flatten_tool_result_content_joins_blocks_with_newlines() {
        let content = vec![
            ToolResultContentBlock::Text {
                text: "line one".to_string(),
            },
            ToolResultContentBlock::Text {
                text: "line two".to_string(),
            },
        ];
        assert_eq!(flatten_tool_result_content(&content), "line one\nline two");
    }

    #[test]
    fn flatten_tool_result_content_handles_single_and_empty() {
        let single = vec![ToolResultContentBlock::Text {
            text: "only".to_string(),
        }];
        assert_eq!(flatten_tool_result_content(&single), "only");
        assert_eq!(flatten_tool_result_content(&[]), "");
    }

    #[test]
    fn flatten_tool_result_content_serializes_json_blocks() {
        let content = vec![ToolResultContentBlock::Json {
            value: json!({"key": "value"}),
        }];
        let result = flatten_tool_result_content(&content);
        assert!(result.contains("key"));
        assert!(result.contains("value"));
    }

    #[test]
    fn translate_message_produces_openai_compatible_shape() {
        let msg = InputMessage::user_text("hello");
        let translated = translate_message(&msg, "gpt-4o");
        assert_eq!(translated.len(), 1);
        assert_eq!(translated[0]["role"], "user");
        assert_eq!(translated[0]["content"], "hello");
    }

    #[test]
    fn build_chat_completion_request_sets_model_and_messages() {
        let request = MessageRequest {
            model: "gpt-4o".to_string(),
            max_tokens: 100,
            messages: vec![InputMessage::user_text("test")],
            stream: false,
            ..Default::default()
        };
        let payload = build_chat_completion_request(&request, OpenAiCompatConfig::openai());
        assert_eq!(payload["model"], "gpt-4o");
        assert!(payload["messages"].is_array());
        assert_eq!(payload["max_tokens"], 100);
    }

    // -----------------------------------------------------------------------
    // PromptCache types via crate-root re-exports
    // -----------------------------------------------------------------------

    #[test]
    fn prompt_cache_config_default_uses_reasonable_values() {
        let config = PromptCacheConfig::default();
        assert_eq!(config.session_id, "default");
        assert!(config.completion_ttl.as_secs() > 0);
        assert!(config.prompt_ttl.as_secs() > 0);
        assert!(config.cache_break_min_drop > 0);
    }

    #[test]
    fn prompt_cache_config_new_accepts_custom_session_id() {
        let config = PromptCacheConfig::new("my-session-42");
        assert_eq!(config.session_id, "my-session-42");
    }

    #[test]
    fn prompt_cache_paths_for_session_produces_consistent_structure() {
        let paths = PromptCachePaths::for_session("test-session");
        assert!(paths.session_dir.ends_with("test-session"));
        assert!(paths.completion_dir.ends_with("completions"));
        assert!(paths.stats_path.ends_with("stats.json"));
        assert!(paths.session_state_path.ends_with("session-state.json"));
    }

    #[test]
    fn prompt_cache_paths_sanitizes_special_characters_in_session_id() {
        let paths = PromptCachePaths::for_session("session:/with spaces!");
        let dir_name = paths
            .session_dir
            .file_name()
            .and_then(|n| n.to_str())
            .expect("session dir should have a name");
        // Special characters should be replaced with hyphens
        assert!(!dir_name.contains('/'));
        assert!(!dir_name.contains(' '));
        assert!(!dir_name.contains('!'));
        assert!(!dir_name.contains(':'));
    }

    #[test]
    fn prompt_cache_stats_default_starts_at_zero() {
        let stats = PromptCacheStats::default();
        assert_eq!(stats.tracked_requests, 0);
        assert_eq!(stats.completion_cache_hits, 0);
        assert_eq!(stats.completion_cache_misses, 0);
        assert_eq!(stats.unexpected_cache_breaks, 0);
        assert!(stats.last_request_hash.is_none());
    }

    #[test]
    fn cache_break_event_serializes_and_deserializes() {
        let event = CacheBreakEvent {
            unexpected: true,
            reason: "prompt fingerprint stable but tokens dropped".to_string(),
            previous_cache_read_input_tokens: 5000,
            current_cache_read_input_tokens: 1000,
            token_drop: 4000,
        };
        let json = serde_json::to_value(&event).expect("serialize");
        assert_eq!(json["unexpected"], true);
        assert_eq!(json["token_drop"], 4000);
        let restored: CacheBreakEvent = serde_json::from_value(json).expect("deserialize");
        assert_eq!(event, restored);
    }

    // -----------------------------------------------------------------------
    // Telemetry re-exports
    // -----------------------------------------------------------------------

    #[test]
    fn client_identity_default_produces_valid_user_agent() {
        let identity = ClientIdentity::default();
        let ua = identity.user_agent();
        assert!(!ua.is_empty());
        assert!(ua.contains('/'));
    }

    #[test]
    fn client_identity_custom_fields_reflected_in_user_agent() {
        let identity = ClientIdentity::new("my-app", "1.2.3");
        assert_eq!(identity.user_agent(), "my-app/1.2.3");
        assert_eq!(identity.app_name, "my-app");
        assert_eq!(identity.app_version, "1.2.3");
    }

    #[test]
    fn anthropic_request_profile_builds_with_betas_and_extra_body() {
        let profile =
            AnthropicRequestProfile::new(ClientIdentity::new("test", "0.1"))
                .with_beta("test-beta-2026")
                .with_extra_body("custom_field", json!(42));

        let headers = profile.header_pairs();
        assert!(
            headers.iter().any(|(k, _)| k == "anthropic-version"),
            "must include anthropic-version header"
        );
        assert!(
            headers.iter().any(|(k, v)| k == "anthropic-beta" && v.contains("test-beta-2026")),
            "must include beta header"
        );
        assert_eq!(profile.extra_body.get("custom_field"), Some(&json!(42)));
    }

    #[test]
    fn default_anthropic_version_is_the_expected_constant() {
        // This ensures the re-export is alive and carries the expected value
        assert_eq!(DEFAULT_ANTHROPIC_VERSION, "2023-06-01");
    }

    #[test]
    fn analytics_event_builder_sets_namespace_action_and_properties() {
        let event = AnalyticsEvent::new("session", "started")
            .with_property("model", json!("claude-sonnet-4-6"))
            .with_property("tokens", json!(1234));

        assert_eq!(event.namespace, "session");
        assert_eq!(event.action, "started");
        assert_eq!(
            event.properties.get("model"),
            Some(&json!("claude-sonnet-4-6"))
        );
        assert_eq!(event.properties.get("tokens"), Some(&json!(1234)));
    }

    #[test]
    fn memory_telemetry_sink_collects_events() {
        use std::sync::Arc;

        let sink = Arc::new(MemoryTelemetrySink::default());
        let event = TelemetryEvent::Analytics(
            AnalyticsEvent::new("test", "ping").with_property("v", json!(1)),
        );
        sink.record(event);

        let events = sink.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            TelemetryEvent::Analytics(e) => {
                assert_eq!(e.namespace, "test");
                assert_eq!(e.action, "ping");
            }
            other => panic!("expected Analytics event, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Cross-module composition: types + providers + sse
    // -----------------------------------------------------------------------

    #[test]
    fn message_request_with_tools_serializes_for_both_anthropic_and_openai() {
        let request = MessageRequest {
            model: "gpt-4o".to_string(),
            max_tokens: 1024,
            messages: vec![InputMessage::user_text("weather in Paris?")],
            tools: Some(vec![ToolDefinition {
                name: "get_weather".to_string(),
                description: Some("Get current weather".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "city": {"type": "string"}
                    }
                }),
            }]),
            tool_choice: Some(ToolChoice::Auto),
            stream: false,
            ..Default::default()
        };

        // Anthropic serialization (native serde)
        let anthropic_json =
            serde_json::to_value(&request).expect("should serialize for Anthropic");
        assert!(anthropic_json["tools"].is_array());

        // OpenAI-compat translation
        let openai_payload = build_chat_completion_request(&request, OpenAiCompatConfig::openai());
        assert!(openai_payload["tools"].is_array());
        assert_eq!(
            openai_payload["tools"][0]["type"],
            json!("function"),
            "OpenAI format wraps tools in function type"
        );
    }

    #[test]
    fn sse_parser_with_context_attaches_provider_and_model_to_parse_errors() {
        let parser = SseParser::new().with_context("TestProvider", "test-model");
        // Feed the parser invalid JSON to trigger an error that should include context
        let mut parser = parser;
        let result = parser.push(b"data: {not valid json}\n\n");
        let error = result.expect_err("invalid JSON should produce an error");
        let rendered = error.to_string();
        assert!(
            rendered.contains("TestProvider"),
            "error should name the provider: {rendered}"
        );
        assert!(
            rendered.contains("test-model"),
            "error should name the model: {rendered}"
        );
    }

    // -----------------------------------------------------------------------
    // ProviderClient enum — variant accessibility (no network)
    // -----------------------------------------------------------------------

    #[test]
    fn provider_client_kind_matches_variant() {
        // We can't construct ProviderClient variants easily without env vars,
        // but we can verify the enum and its associated types are accessible
        // through the crate root. The type must be nameable.
        fn _assert_provider_client_is_importable(_: &ProviderClient) {}

        // PromptCacheRecord is composed from sub-module types
        fn _assert_prompt_cache_record_is_importable(_: &PromptCacheRecord) {}
    }
}
