use super::{
    model::{Page, TunnelTrafficResp},
    routes::{Dashboard, query_param, text, traffic_resp, traffic_window},
};
use courierust::{
    courierust_body::Body,
    courierust_http::{response::Response, status::StatusCode},
};
use std::fmt::{Display, Write};

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn encode(s: &str) -> String {
    let mut out = String::new();
    for byte in s.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            write!(out, "%{byte:02X}").unwrap();
        }
    }
    out
}

struct View<'a> {
    path: &'a str,
    query: &'a str,
    zh: bool,
    theme: &'static str,
}
impl View<'_> {
    fn t<'a>(&self, en: &'a str, zh: &'a str) -> &'a str {
        if self.zh { zh } else { en }
    }
    fn value(&self, key: &str) -> String {
        query_param(self.query, key).unwrap_or_default()
    }
    fn url(&self, key: &str, value: &str) -> String {
        let mut pairs = vec![format!("{key}={}", encode(value))];
        for k in [
            "id",
            "name",
            "q",
            "sessionId",
            "protocol",
            "status",
            "page",
            "pageSize",
            "range",
            "chart",
            "live",
        ] {
            if k != key
                && !(k == "page" && matches!(key, "status" | "protocol" | "pageSize"))
                && let Some(v) = query_param(self.query, k)
            {
                pairs.push(format!("{k}={}", encode(&v)));
            }
        }
        format!("{}?{}", self.path, pairs.join("&amp;"))
    }
    fn link(&self, key: &str, value: &str, label: &str) -> String {
        format!("<a href='{}'>{}</a>", self.url(key, value), escape(label))
    }
    fn pager<T: nextjson::NsonSerialize>(&self, p: &Page<T>) -> String {
        let pages = p.total.div_ceil(p.page_size).max(1);
        let mut nav = format!(
            "<nav>{} {} / {} · {} {} ",
            self.t("Page", "页"),
            p.page,
            pages,
            p.total,
            self.t("results", "条结果")
        );
        if p.page > 1 {
            nav += &self.link(
                "page",
                &(p.page - 1).to_string(),
                self.t("Previous", "上一页"),
            );
        }
        if p.page < pages {
            nav += &self.link("page", &(p.page + 1).to_string(), self.t("Next", "下一页"));
        }
        for size in [10, 50, 200] {
            nav += &self.link("pageSize", &size.to_string(), &size.to_string());
        }
        nav + "</nav>"
    }
    fn traffic(&self, history: TunnelTrafficResp) -> String {
        let mut out = format!(
            "<h2>{}</h2><nav>{} {} {} {}</nav>",
            self.t("Traffic history", "流量历史"),
            self.link("range", "24h", "24 h"),
            self.link("range", "7d", "7 d"),
            self.link("chart", "bar", self.t("Bars", "柱状图")),
            self.link("chart", "line", self.t("Lines", "折线图"))
        );
        let peak = history
            .history
            .iter()
            .map(|p| p.traffic_in.max(p.traffic_out))
            .max()
            .unwrap_or(0)
            .max(1);
        let mut points = [String::new(), String::new()];
        let width = history.history.len().max(1) * 16;
        let mut chart =
            format!("<svg viewBox='0 0 {width} 60' aria-hidden='true' preserveAspectRatio='none'>");
        let line = self.value("chart") == "line";
        for (i, p) in history.history.iter().enumerate() {
            for (j, v) in [p.traffic_in, p.traffic_out].into_iter().enumerate() {
                let height = v as f64 / peak as f64 * 58.;
                let y = 60. - height;
                let x = i * 16 + j * 6 + 1;
                if line {
                    write!(points[j], "{},{} ", i * 16 + 6, y).unwrap();
                } else {
                    write!(
                        chart,
                        "<rect class='s{j}' x='{x}' y='{y}' width='5' height='{height}'/>"
                    )
                    .unwrap();
                }
            }
        }
        if line {
            for (j, p) in points.iter().enumerate() {
                write!(chart, "<polyline class='l{j}' points='{p}'/>").unwrap();
            }
        }
        out += &chart;
        out += "</svg>";
        out += &table(&[
            self.t("Time", "时间"),
            self.t("Inbound (bytes)", "入站（字节）"),
            self.t("Outbound (bytes)", "出站（字节）"),
        ]);
        for p in history.history {
            write!(
                out,
                "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape(&p.date),
                p.traffic_in,
                p.traffic_out
            )
            .unwrap();
        }
        out + "</tbody></table></div>"
    }
}

fn table(labels: &[&str]) -> String {
    let mut out = String::from("<div class='scroll'><table><thead><tr>");
    for label in labels {
        write!(out, "<th>{}</th>", escape(label)).unwrap();
    }
    out + "</tr></thead><tbody>"
}
fn field(out: &mut String, key: &str, value: impl Display) {
    write!(
        out,
        "<dt>{}</dt><dd>{}</dd>",
        escape(key),
        escape(&value.to_string())
    )
    .unwrap();
}

impl Dashboard {
    pub(super) fn disconnect_view(&self, query: &str) -> Response<Body> {
        let id = query_param(query, "id").unwrap_or_default();
        match self.blocking(self.state.svc.kick_client(&id)) {
            Ok(()) => Response::new(StatusCode::SEE_OTHER)
                .header("location", "/clients")
                .with_body(Body::from("")),
            Err(_) => text(StatusCode::NOT_FOUND, "client offline"),
        }
    }

    pub(super) fn view(&self, path: &str, query: &str, cookies: &str) -> Response<Body> {
        if !matches!(path, "/" | "/clients" | "/client" | "/tunnels" | "/tunnel") {
            return text(StatusCode::NOT_FOUND, "not found");
        }
        let cookie = cookies
            .split(';')
            .find_map(|p| p.trim().strip_prefix("astrave_view="))
            .unwrap_or("en-auto");
        let (lang, theme) = cookie.split_once('-').unwrap_or(("en", "auto"));
        let zh = query_param(query, "lang").as_deref().unwrap_or(lang) == "zh";
        let theme = match query_param(query, "theme").as_deref().unwrap_or(theme) {
            "light" => "light",
            "dark" => "dark",
            _ => "auto",
        };
        let v = View {
            path,
            query,
            zh,
            theme,
        };
        let title = match path {
            "/" => v.t("Overview", "概览"),
            "/clients" => v.t("Clients", "客户端"),
            "/client" => v.t("Client details", "客户端详情"),
            "/tunnels" => v.t("Tunnels", "隧道"),
            _ => v.t("Tunnel details", "隧道详情"),
        };
        let live = v.value("live") == "1";
        let lang = v.t("en", "zh");
        let mut html = format!(
            "<!doctype html><html lang='{lang}' class='{theme}'><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'><title>{title} · Astrave</title><style>{}</style>",
            include_str!("dashboard.css")
        );
        if live {
            html.push_str("<meta http-equiv='refresh' content='5'>");
        }
        write!(html,"<header><b><img src='/favicon.ico' width='28' height='28' alt=''> Astrave</b><nav><a href='/'>{}</a><a href='/clients'>{}</a><a href='/tunnels'>{}</a></nav></header><main><h1>{title}</h1>",v.t("Overview","概览"),v.t("Clients","客户端"),v.t("Tunnels","隧道")).unwrap();
        match path {
            "/" => {
                let info = self.system_info().data;
                let s = info.status;
                let c = info.config;
                html.push_str("<dl>");
                field(
                    &mut html,
                    v.t("Clients online / total", "在线 / 全部客户端"),
                    format!("{} / {}", s.client_counts, s.total_client_counts),
                );
                field(
                    &mut html,
                    v.t("Active connections", "活动连接"),
                    s.active_connections,
                );
                field(
                    &mut html,
                    v.t("Inbound today (bytes)", "今日入站（字节）"),
                    s.total_traffic_in,
                );
                field(
                    &mut html,
                    v.t("Outbound today (bytes)", "今日出站（字节）"),
                    s.total_traffic_out,
                );
                for (ty, n) in s.tunnel_type_count {
                    field(&mut html, &ty, n);
                }
                html.push_str("</dl>");
                write!(
                    html,
                    "<details><summary>{}</summary><dl>",
                    v.t("Server configuration", "服务端配置")
                )
                .unwrap();
                field(&mut html, "Astrave", info.version);
                field(&mut html, "listen", c.listen);
                field(&mut html, "quicPort", c.quic_port);
                field(&mut html, "kcpPort", c.kcp_port);
                field(&mut html, "httpGwPort", c.http_gw_port);
                field(&mut html, "httpsGwPort", c.https_gw_port);
                field(&mut html, "rootDomain", c.root_domain);
                field(&mut html, "tcpMux", c.tcp_mux);
                field(&mut html, "tlsForce", c.tls_force);
                field(&mut html, "maxConnPool", c.max_conn_pool);
                field(&mut html, "heartbeatTimeout", c.heartbeat_timeout);
                html.push_str("</dl></details>");
                html += &v.traffic(self.system_traffic(query).data);
            }
            "/clients" => {
                let page = self.list_clients(query).data;
                write!(
                    html,
                    "<nav>{} {} {}</nav>",
                    v.link("status", "", v.t("All", "全部")),
                    v.link("status", "online", v.t("Online", "在线")),
                    v.link("status", "offline", v.t("Offline", "离线"))
                )
                .unwrap();
                html += &table(&[
                    v.t("Client", "客户端"),
                    v.t("Hostname", "主机名"),
                    "IP",
                    v.t("Status", "状态"),
                    v.t("Tunnels", "隧道"),
                ]);
                for c in &page.items {
                    write!(html,"<tr><td><a href='/client?id={}'>{}</a></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",encode(&c.session_id),escape(c.display_id()),escape(&c.hostname),escape(&c.client_ip),escape(&c.status),c.tunnel_count).unwrap();
                }
                html.push_str("</tbody></table></div>");
                html += &v.pager(&page);
            }
            "/client" => {
                let id = v.value("id");
                let Some(c) = self
                    .blocking(self.state.svc.dashboard_clients(Some(&id), "", 1, 1))
                    .items
                    .pop()
                else {
                    return text(StatusCode::NOT_FOUND, "client not found");
                };
                html.push_str("<dl>");
                for (label, value) in [
                    ("Session", &c.session_id),
                    ("Agent", &c.agent_id),
                    (v.t("User", "用户"), &c.user),
                    (v.t("Hostname", "主机名"), &c.hostname),
                    ("OS", &c.os),
                    ("CPU", &c.arch),
                    ("IP", &c.client_ip),
                    (v.t("Version", "版本"), &c.version),
                    (v.t("Status", "状态"), &c.status),
                ] {
                    field(&mut html, label, value);
                }
                field(
                    &mut html,
                    v.t("Seconds in current state", "当前状态时长（秒）"),
                    c.connected_secs,
                );
                field(
                    &mut html,
                    v.t("Active connections", "活动连接"),
                    c.active_connections,
                );
                field(&mut html, v.t("Tunnels", "隧道"), c.tunnel_count);
                let session = encode(&c.session_id);
                write!(
                    html,
                    "</dl><a href='/tunnels?sessionId={session}'>{}</a>",
                    v.t("View client tunnels", "查看客户端隧道")
                )
                .unwrap();
                if c.status == "online" {
                    write!(html,"<details><summary>{}</summary><form method='post' action='/disconnect?id={session}'><button>{}</button></form></details>",v.t("Disconnect client","断开客户端"),v.t("Confirm disconnect","确认断开")).unwrap();
                }
            }
            "/tunnels" => {
                let page = self.list_tunnels(query).data;
                write!(html,"<form><input type=hidden name=sessionId value='{}'><input type=hidden name=protocol value='{}'><label>{} <input type=search name=q value='{}'></label><button>{}</button></form><nav>",escape(&v.value("sessionId")),escape(&v.value("protocol")),v.t("Tunnel name","隧道名称"),escape(&v.value("q")),v.t("Search","搜索")).unwrap();
                html += &v.link("protocol", "", v.t("All", "全部"));
                for ty in ["tcp", "udp", "http", "https"] {
                    html += &v.link("protocol", ty, &ty.to_ascii_uppercase());
                }
                html += "</nav>";
                html += &table(&[
                    v.t("Name", "名称"),
                    v.t("Protocol", "协议"),
                    v.t("Public endpoint", "映射端点"),
                    v.t("Local service", "本地服务"),
                    v.t("Connections", "连接数"),
                ]);
                for t in &page.items {
                    write!(html,"<tr><td><a href='/tunnel?name={}'>{}</a></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",encode(&t.name),escape(&t.name),escape(&t.tunnel_type),escape(&t.remote_addr),escape(&t.local_addr),t.active_connections).unwrap();
                }
                html.push_str("</tbody></table></div>");
                html += &v.pager(&page);
            }
            _ => {
                let name = v.value("name");
                let svc = &self.state.svc;
                let t = svc
                    .tunnel_registry
                    .owner(&name)
                    .and_then(|owner| self.blocking(svc.dashboard_tunnel(&name, owner)));
                let Some(t) = t else {
                    return text(StatusCode::NOT_FOUND, "tunnel not found");
                };
                html.push_str("<dl>");
                field(&mut html, v.t("Name", "名称"), &t.name);
                field(&mut html, v.t("Protocol", "协议"), t.tunnel_type);
                field(&mut html, v.t("Public endpoint", "映射端点"), t.remote_addr);
                field(&mut html, v.t("Local service", "本地服务"), t.local_addr);
                field(&mut html, v.t("Status", "状态"), t.status);
                field(
                    &mut html,
                    v.t("Active connections", "活动连接"),
                    t.active_connections,
                );
                field(
                    &mut html,
                    v.t("Inbound today (bytes)", "今日入站（字节）"),
                    t.today_traffic_in,
                );
                field(
                    &mut html,
                    v.t("Outbound today (bytes)", "今日出站（字节）"),
                    t.today_traffic_out,
                );
                field(
                    &mut html,
                    v.t("Last start", "上次启动"),
                    t.last_start_time.unwrap_or_default(),
                );
                write!(
                    html,
                    "</dl><a href='/client?id={}'>{}</a>",
                    encode(&t.session_id),
                    escape(&t.session_id)
                )
                .unwrap();
                if let Some(hist) = svc.metrics().tunnel_traffic(&name, traffic_window(query)) {
                    html += &v.traffic(traffic_resp(hist));
                }
            }
        }
        write!(
            html,
            "<footer><nav>{} {} {} {}</nav></footer></main></html>",
            v.link("lang", v.t("zh", "en"), v.t("简体中文", "English")),
            v.link(
                "theme",
                if v.theme == "dark" { "light" } else { "dark" },
                if v.theme == "dark" {
                    v.t("Light", "浅色")
                } else {
                    v.t("Dark", "深色")
                }
            ),
            v.link("theme", "auto", v.t("System theme", "系统主题")),
            v.link(
                "live",
                if live { "0" } else { "1" },
                if live {
                    v.t("Stop refresh", "停止刷新")
                } else {
                    v.t("Refresh every 5 s", "每 5 秒刷新")
                }
            )
        )
        .unwrap();
        Response::new(StatusCode::OK).header("content-type","text/html; charset=utf-8").header("cache-control","no-store").header("content-security-policy","default-src 'none'; style-src 'unsafe-inline'; img-src 'self'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'").header("x-content-type-options","nosniff").header("referrer-policy","same-origin").header("set-cookie",format!("astrave_view={lang}-{theme}; Path=/; SameSite=Strict; HttpOnly; Max-Age=31536000")).with_body(Body::from(html))
    }
}
