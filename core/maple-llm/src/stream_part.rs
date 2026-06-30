use serde::{Deserialize, Serialize}; use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StreamPart {
    TextDelta { text: String },
    ToolCallStart { id: String, name: String },
    ToolCallDelta { id: String, args_chunk: String },
    ToolCallEnd { id: String, args: Value },
    ReasoningDelta { text: String },
    ArtifactStart { id: String, kind: String },
    ArtifactDelta { id: String, content_chunk: String },
    ArtifactEnd { id: String },
    Usage { input_tokens: u32, output_tokens: u32, cost_usd: f64 },
    Done,
    Error { message: String },
}

impl StreamPart {
    pub fn to_sse_event(&self) -> String {
        let t = match self { StreamPart::TextDelta{..}=>"text_delta", StreamPart::ToolCallStart{..}=>"tool_call_start", StreamPart::ToolCallDelta{..}=>"tool_call_delta", StreamPart::ToolCallEnd{..}=>"tool_call_end", StreamPart::ReasoningDelta{..}=>"reasoning_delta", StreamPart::ArtifactStart{..}=>"artifact_start", StreamPart::ArtifactDelta{..}=>"artifact_delta", StreamPart::ArtifactEnd{..}=>"artifact_end", StreamPart::Usage{..}=>"usage", StreamPart::Done=>"done", StreamPart::Error{..}=>"error" };
        format!("event: {t}\ndata: {}\n\n", serde_json::to_string(self).unwrap_or_default())
    }
}

pub struct PartialJsonParser { buffer: String, depth: i32, in_string: bool, escape_next: bool }
impl Default for PartialJsonParser { fn default() -> Self { Self::new() } }
impl PartialJsonParser {
    pub fn new() -> Self { Self { buffer: String::new(), depth: 0, in_string: false, escape_next: false } }
    pub fn feed(&mut self, chunk: &str) -> Option<Value> {
        for ch in chunk.chars() {
            self.buffer.push(ch);
            if self.escape_next { self.escape_next = false; continue; }
            if self.in_string { match ch { '\\'=>self.escape_next=true, '"'=>self.in_string=false, _=>{} } continue; }
            match ch { '"'=>self.in_string=true, '{'|'['=>self.depth+=1, '}'|']'=>self.depth-=1, _=>{} }
        }
        if self.buffer.trim().is_empty() { return None; }
        if let Ok(v) = serde_json::from_str::<Value>(&self.buffer) { return Some(v); }
        let mut p = self.buffer.clone();
        while p.ends_with(',') || p.ends_with(' ') { p.pop(); }
        for _ in 0..(p.matches('{').count() as i32 - p.matches('}').count() as i32).max(0) { p.push('}'); }
        for _ in 0..(p.matches('[').count() as i32 - p.matches(']').count() as i32).max(0) { p.push(']'); }
        if self.in_string { p.push('"'); }
        serde_json::from_str::<Value>(&p).ok()
    }
    pub fn reset(&mut self) { self.buffer.clear(); self.depth = 0; self.in_string = false; self.escape_next = false; }
}

#[derive(Default)]
pub struct ToolCallBuffer { calls: std::collections::HashMap<String, (String, String)> }
impl ToolCallBuffer {
    pub fn start(&mut self, id: &str, name: &str) { self.calls.insert(id.into(), (name.into(), String::new())); }
    pub fn append(&mut self, id: &str, c: &str) { if let Some(e) = self.calls.get_mut(id) { e.1.push_str(c); } }
    pub fn finalize_all(&mut self) -> Vec<(String, String)> { std::mem::take(&mut self.calls).into_iter().map(|(id,(_,a))|(id,a)).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_sse() { let p = StreamPart::TextDelta{text:"hi".into()}; assert!(p.to_sse_event().contains("text_delta")); }
    #[test] fn test_json_complete() { let mut p = PartialJsonParser::new(); assert!(p.feed(r#"{"a":1}"#).is_some()); }
    #[test] fn test_buffer() { let mut b = ToolCallBuffer::default(); b.start("c1","search"); b.append("c1",r#"{"q":""#); b.append("c1",r#"hi"}"#); let f = b.finalize_all(); assert_eq!(f.len(), 1); }
}
