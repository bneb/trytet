use crossterm::event::{Event, KeyCode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Row, Table},
    Frame,
};
use crate::models::TetExecutionResult;
use crate::market::HiveMarket;

    fn process_event(&mut self, event: HiveEvent) {
        self.total_events += 1;

        match event {
            HiveEvent::AgentBooted {
                tet_id,
                alias,
                fuel_limit,
                memory_limit_mb,
                timestamp_us,
            } => {
                let display_alias = alias.clone().unwrap_or_else(|| tet_id[..8].to_string());
                self.agents.insert(
                    tet_id.clone(),
                    AgentState {
                        _tet_id: tet_id.clone(),
                        alias: display_alias.clone(),
                        status: "Running".to_string(),
                        fuel_consumed: 0,
                        fuel_limit,
                        memory_kb: memory_limit_mb as u64 * 1024,
                        last_updated_us: timestamp_us,
                    },
                );
                self.push_event_log(format!(
                    "▶  BOOT  {} (fuel: {}, mem: {}MB)",
                    display_alias, fuel_limit, memory_limit_mb
                ));
            }

            HiveEvent::AgentCompleted {
                tet_id,
                status,
                fuel_consumed,
                fuel_limit,
                memory_used_kb,
                duration_us,
                timestamp_us,
                ..
            } => {
                if let Some(agent) = self.agents.get_mut(&tet_id) {
                    agent.status = status.clone();
                    agent.fuel_consumed = fuel_consumed;
                    agent.memory_kb = memory_used_kb;
                    agent.last_updated_us = timestamp_us;
                }
                let alias = self
                    .agents
                    .get(&tet_id)
                    .map(|a| a.alias.clone())
                    .unwrap_or_else(|| tet_id[..8].to_string());
                self.push_event_log(format!(
                    "■  DONE  {} → {} ({:.1}ms, {}/{} fuel)",
                    alias,
                    status,
                    duration_us as f64 / 1000.0,
                    fuel_consumed,
                    fuel_limit
                ));
            }

            HiveEvent::FuelConsumed {
                tet_id,
                operation,
                amount,
                ..
            } => {
                if let Some(agent) = self.agents.get_mut(&tet_id) {
                    agent.fuel_consumed += amount;
                }
                let alias = self
                    .agents
                    .get(&tet_id)
                    .map(|a| a.alias.clone())
                    .unwrap_or_else(|| tet_id[..8].to_string());
                self.push_event_log(format!("⚡ FUEL  {} -{} ({})", alias, amount, operation));
            }

            HiveEvent::TeleportInitiated {
                agent_id,
                target_node,
                ..
            } => {
                if let Some(agent) = self.agents.get_mut(&agent_id) {
                    agent.status = "MIGRATING".to_string();
                }
                self.push_event_log(format!("✈  TELE  {} → {}", agent_id, target_node));
            }

            HiveEvent::TeleportCompleted {
                agent_id,
                target_node,
                bytes_transferred,
                ..
            } => {
                self.push_event_log(format!(
                    "✓  LAND  {} @ {} ({:.1}KB)",
                    agent_id,
                    target_node,
                    bytes_transferred as f64 / 1024.0
                ));
            }

            HiveEvent::OracleHit {
                tet_id,
                request_hash,
                ..
            } => {
                self.oracle.cache_hits += 1;
                let alias = self
                    .agents
                    .get(&tet_id)
                    .map(|a| a.alias.clone())
                    .unwrap_or_else(|| "?".to_string());
                self.push_event_log(format!("◉  HIT   {} hash:{}…", alias, &request_hash[..12]));
            }

            HiveEvent::OracleMiss { tet_id, url, .. } => {
                self.oracle.cache_misses += 1;
                let alias = self
                    .agents
                    .get(&tet_id)
                    .map(|a| a.alias.clone())
                    .unwrap_or_else(|| "?".to_string());
                self.push_event_log(format!("○  MISS  {} → {}", alias, url));
            }

            HiveEvent::InferenceStarted {
                tet_id,
                model_id,
                prompt_tokens_est,
                ..
            } => {
                let alias = self
                    .agents
                    .get(&tet_id)
                    .map(|a| a.alias.clone())
                    .unwrap_or_else(|| "guest".to_string());
                self.push_event_log(format!(
                    "🧠 INFER {} → {} (~{}tok)",
                    alias, model_id, prompt_tokens_est
                ));
            }

            HiveEvent::InferenceCompleted {
                model_id,
                input_tokens,
                output_tokens,
                fuel_cost,
                cached,
                ..
            } => {
                self.oracle.total_inferences += 1;
                if cached {
                    self.oracle.cached_inferences += 1;
                }
                let tag = if cached { "CACHED" } else { "FRESH" };
                self.push_event_log(format!(
                    "🧠 {}  {} in:{}→out:{} (fuel:{})",
                    tag, model_id, input_tokens, output_tokens, fuel_cost
                ));
            }

            HiveEvent::ContextPruned {
                tet_id,
                tokens_removed,
                blocks_evicted,
                ..
            } => {
                let alias = self
                    .agents
                    .get(&tet_id)
                    .map(|a| a.alias.clone())
                    .unwrap_or_else(|| "?".to_string());
                self.push_event_log(format!(
                    "✂  PRUNE {} -{} tokens ({} blocks)",
                    alias, tokens_removed, blocks_evicted
                ));
            }
        }
    }
