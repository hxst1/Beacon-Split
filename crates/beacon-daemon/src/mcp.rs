//! The MCP server Claude talks to when it has something for you to copy.
//!
//! Runs as a child of Claude Code, one process per session, speaking JSON-RPC
//! over stdin and stdout. It owns no state: a tool call turns into one line on
//! the daemon's socket and a reply back.
//!
//! It knows which project it belongs to without being told, and that is the
//! whole trick. Beacon starts every Claude session with `BEACON_SOCKET` and
//! `BEACON_PROJECT` in its environment (see `session::spawn`), an MCP server is
//! a child of that session, and children inherit the environment. So there is
//! no configuration file to write, no port to agree on, no token to exchange,
//! and nothing for the user to connect — and Claude never has to be told where
//! it is working, because the transport already knows.
//!
//! Outside Beacon there is no socket, so it advertises no tools and does
//! nothing. Same rule as the hook, for the same reason: a thing registered once
//! must be harmless everywhere else.
//!
//! The protocol is hand-written rather than pulled from a crate. It is four
//! methods, the daemon's own wire format is hand-written newline JSON for the
//! same reasons, and an MCP crate would be by far the largest dependency in a
//! binary that has to stay small enough to ship inside an app bundle.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use beacon_core::clips::ClipKind;
use beacon_core::domain::ProjectId;
use beacon_core::protocol::{Envelope, Message, Outcome, Request};
use serde_json::{Value, json};

/// The MCP revisions this server knows how to be.
///
/// A client asking for one of these is answered in its own version; anything
/// else is answered in our newest and left to decide. Listed newest first.
const SUPPORTED: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// How long to wait for the daemon to confirm a clip.
///
/// Short on purpose. This runs inside someone's turn: a daemon that has wedged
/// must cost them a message saying so, not a minute of a spinner.
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// What Claude is told the tool is for.
///
/// Worth as much care as any code here: it is the entire mechanism by which the
/// feature fires at the right moment. It names the artefacts rather than the
/// occasion, because "when the user wants to copy something" is a judgement and
/// "an environment variable, a command, an email" is a recognition.
///
/// The last line exists because without it the tool gets called *instead of*
/// answering, and the user is left looking at a turn that did nothing.
const TOOL_DESCRIPTION: &str = "\
Put a short piece of text in Beacon's clip drawer, where the user can copy it \
with one click.

Use this whenever you produce something the user is going to paste somewhere \
outside this conversation: an environment variable or block of them, a command \
to run on another machine, the body of an email or a message to a person, a \
URL, a token, a config snippet.

Send the exact text to paste and nothing else — no explanation, no surrounding \
prose, and no markdown code fences. Answer the user normally as well: this is \
in addition to your reply, not instead of it.";

pub fn run() -> ! {
    let target = Target::from_environment();

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }

        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            // A line we cannot parse has no id to answer against, so there is
            // nobody to tell. Claude Code owns this pipe; dropping it is right.
            continue;
        };

        // A notification has no id and must never be answered. Replying to one
        // is a protocol violation that some clients treat as fatal.
        let Some(id) = message.get("id").cloned() else {
            continue;
        };

        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = message.get("params").cloned().unwrap_or(Value::Null);

        let response = match method {
            "initialize" => ok(id, initialize(&params)),
            "tools/list" => ok(id, json!({ "tools": target.tools() })),
            "tools/call" => ok(id, target.call(&params)),
            // Answered because a client that pings and hears nothing concludes
            // the server is dead and tears it down mid-session.
            "ping" => ok(id, json!({})),
            other => error(id, -32601, &format!("unknown method: {other}")),
        };

        if write_line(&mut stdout, &response).is_err() {
            break;
        }
    }

    std::process::exit(0)
}

fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = asked
        .filter(|version| SUPPORTED.contains(version))
        .unwrap_or(SUPPORTED[0]);

    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "beacon", "version": env!("CARGO_PKG_VERSION") },
    })
}

/// Where clips go, when there is anywhere for them to go.
struct Target {
    socket: Option<String>,
    project: Option<ProjectId>,
}

impl Target {
    fn from_environment() -> Self {
        Self {
            socket: std::env::var("BEACON_SOCKET").ok(),
            project: std::env::var("BEACON_PROJECT").ok().map(ProjectId),
        }
    }

    fn reachable(&self) -> Option<(&str, &ProjectId)> {
        Some((self.socket.as_deref()?, self.project.as_ref()?))
    }

    /// The tool, or nothing at all outside Beacon.
    ///
    /// Nothing rather than a tool that always fails: an advertised tool costs
    /// context in every single turn, and one that cannot work is a tax with no
    /// upside. This is also what makes the server safe to leave registered.
    fn tools(&self) -> Value {
        if self.reachable().is_none() {
            return json!([]);
        }

        json!([{
            "name": "save_clip",
            "description": TOOL_DESCRIPTION,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description":
                            "A short label, two to five words, so the user can recognise this \
                             in a list. Not a sentence.",
                    },
                    "body": {
                        "type": "string",
                        "description":
                            "The exact text to put on the clipboard. No code fences, no \
                             commentary, no leading prompt characters like $.",
                    },
                    "kind": {
                        "type": "string",
                        "enum": ["text", "command", "variable", "email"],
                        "description":
                            "What it is, so the drawer can show it correctly. Use 'command' for \
                             something to run in a shell, 'variable' for environment values, \
                             'email' for prose written to a person, 'text' otherwise.",
                    },
                },
                "required": ["title", "body"],
            },
        }])
    }

    fn call(&self, params: &Value) -> Value {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if name != "save_clip" {
            return tool_error(format!("there is no tool called {name}"));
        }

        let Some((socket, project)) = self.reachable() else {
            return tool_error(
                "Beacon is not running this session, so there is no drawer to put this in. \
                 Give the text to the user in your reply instead."
                    .to_string(),
            );
        };

        let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
        let title = arguments
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let body = arguments
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let kind = match arguments.get("kind").and_then(Value::as_str) {
            Some("command") => ClipKind::Command,
            Some("variable") => ClipKind::Variable,
            Some("email") => ClipKind::Email,
            _ => ClipKind::Text,
        };

        // The daemon does the validating, so there is one set of rules and the
        // message Claude reads is the message the rules actually produced.
        match send(
            socket,
            Request::Clip {
                project: project.clone(),
                title: title.to_string(),
                body: body.to_string(),
                kind,
            },
        ) {
            Ok(()) => tool_ok(format!(
                "Saved to the clip drawer as \"{}\". Tell the user it is there.",
                title.trim()
            )),
            Err(reason) => tool_error(reason),
        }
    }
}

/// Sends one request and waits for its reply.
///
/// Skips anything that is not the answer: the daemon broadcasts every clip to
/// every attached client, and this process is one of them, so its own clip
/// arrives as an event on this same socket before the reply does.
fn send(socket: &str, request: Request) -> Result<(), String> {
    let line = serde_json::to_string(&Envelope { id: 1, request })
        .map_err(|err| format!("could not encode the clip: {err}"))?;

    let mut stream = UnixStream::connect(socket)
        .map_err(|_| "Beacon is not listening; it may have been closed.".to_string())?;
    stream
        .set_read_timeout(Some(REPLY_TIMEOUT))
        .map_err(|err| format!("could not set a timeout: {err}"))?;

    stream
        .write_all(line.as_bytes())
        .and_then(|_| stream.write_all(b"\n"))
        .and_then(|_| stream.flush())
        .map_err(|err| format!("could not reach Beacon: {err}"))?;

    let reader = BufReader::new(
        stream
            .try_clone()
            .map_err(|err| format!("could not read from Beacon: {err}"))?,
    );

    for line in reader.lines() {
        let Ok(line) = line else { break };
        match serde_json::from_str::<Message>(&line) {
            Ok(Message::Response(response)) if response.id == 1 => {
                return match response.outcome {
                    Outcome::Ok(_) => Ok(()),
                    Outcome::Err(reason) => Err(reason),
                };
            }
            // Our own clip coming back as a broadcast, or another session's.
            _ => continue,
        }
    }

    Err("Beacon did not confirm the clip.".to_string())
}

fn tool_ok(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": false })
}

/// A failure the model should see and act on, rather than a transport fault.
///
/// Returned as a successful result with `isError`, which is what puts the text
/// in front of Claude. A JSON-RPC error would be handled by the client and
/// Claude would never learn that the body was too long to be worth pasting.
fn tool_error(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": true })
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn write_line(out: &mut impl Write, value: &Value) -> std::io::Result<()> {
    let line = serde_json::to_string(value)?;
    out.write_all(line.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outside_beacon() -> Target {
        Target {
            socket: None,
            project: None,
        }
    }

    fn inside_beacon() -> Target {
        Target {
            socket: Some("/tmp/nowhere.sock".into()),
            project: Some(ProjectId("pj_x".into())),
        }
    }

    #[test]
    fn a_client_is_answered_in_the_version_it_asked_for() {
        let result = initialize(&json!({ "protocolVersion": "2024-11-05" }));
        assert_eq!(result["protocolVersion"], "2024-11-05");
    }

    #[test]
    fn a_version_we_do_not_know_is_answered_in_ours_rather_than_echoed() {
        // Echoing it back would claim to speak something we have never seen.
        let result = initialize(&json!({ "protocolVersion": "1999-01-01" }));
        assert_eq!(result["protocolVersion"], SUPPORTED[0]);
        let absent = initialize(&json!({}));
        assert_eq!(absent["protocolVersion"], SUPPORTED[0]);
    }

    #[test]
    fn outside_beacon_no_tool_is_advertised_at_all() {
        // An advertised tool costs context on every turn of every session,
        // including every session that has nothing to do with Beacon.
        assert_eq!(outside_beacon().tools(), json!([]));
    }

    #[test]
    fn inside_beacon_the_tool_requires_a_title_and_a_body() {
        let tools = inside_beacon().tools();
        let tool = &tools[0];
        assert_eq!(tool["name"], "save_clip");
        assert_eq!(tool["inputSchema"]["required"], json!(["title", "body"]));
        // `kind` must stay optional: a Claude that omits it should still land a
        // clip rather than fail a required-field check.
        assert!(tool["inputSchema"]["properties"]["kind"].is_object());
    }

    #[test]
    fn the_description_tells_claude_to_answer_as_well_as_clip() {
        // Without this the tool call replaces the reply and the user sees a
        // turn that appears to have done nothing.
        let tools = inside_beacon().tools();
        let description = tools[0]["description"].as_str().unwrap();
        assert!(
            description.contains("in addition to your reply"),
            "{description}"
        );
        assert!(
            description.contains("no markdown code fences"),
            "{description}"
        );
    }

    #[test]
    fn calling_an_unknown_tool_is_an_error_claude_can_read() {
        let result = inside_beacon().call(&json!({ "name": "something_else" }));
        assert_eq!(result["isError"], true);
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("something_else")
        );
    }

    #[test]
    fn outside_beacon_a_call_says_what_to_do_instead() {
        let result = outside_beacon().call(&json!({
            "name": "save_clip",
            "arguments": { "title": "t", "body": "b" }
        }));
        assert_eq!(result["isError"], true);
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("in your reply"), "{text}");
    }

    #[test]
    fn a_daemon_that_is_not_there_fails_the_call_rather_than_hanging() {
        let result = inside_beacon().call(&json!({
            "name": "save_clip",
            "arguments": { "title": "t", "body": "b", "kind": "command" }
        }));
        assert_eq!(result["isError"], true);
    }

    #[test]
    fn an_unknown_method_is_a_jsonrpc_error() {
        let response = error(json!(3), -32601, "unknown method: nope");
        assert_eq!(response["error"]["code"], -32601);
        assert_eq!(response["id"], 3);
        assert!(response.get("result").is_none());
    }

    #[test]
    fn a_response_is_one_line_so_the_framing_survives() {
        // Newline-delimited JSON: a pretty-printed reply would be read as
        // several malformed messages.
        let mut out = Vec::new();
        write_line(&mut out, &ok(json!(1), json!({ "a": { "b": 1 } }))).unwrap();
        let written = String::from_utf8(out).unwrap();
        assert_eq!(written.matches('\n').count(), 1);
        assert!(written.ends_with('\n'));
    }
}
