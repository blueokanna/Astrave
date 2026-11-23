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

    fn option(&self, key: &str, value: &str, label: &str, selected: bool) -> String {
        let current = if selected { " aria-current='true'" } else { "" };
        format!(
            "<a href='{}'{current}>{}</a>",
            self.url(key, value),
            escape(label)
        )
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
        let hourly = history.granularity == "hour";
        let line = self.value("chart") == "line";
        let mut out = format!(
            "<section class='panel'><div class='panel-head'><h2>{}</h2><div class='controls'><nav class='segments' aria-label='{}'>{}{}</nav><nav class='segments' aria-label='{}'>{}{}</nav></div></div>",
            self.t("Traffic history", "流量历史"),
            self.t("Time range", "时间范围"),
            self.option("range", "24h", self.t("24 hours", "24 小时"), hourly),
            self.option("range", "7d", self.t("7 days", "7 天"), !hourly),
            self.t("Chart type", "图表类型"),
            self.option("chart", "bar", self.t("Bars", "柱状图"), !line),
            self.option("chart", "line", self.t("Lines", "折线图"), line)
        );
        let peak = history
            .history
            .iter()
            .map(|p| p.traffic_in.max(p.traffic_out))
            .max()
            .unwrap_or(0);
        write!(
            out,
            "<div class='legend'><span class='in'>{}</span><span class='out'>{}</span></div>",
            self.t("Inbound", "入站"),
            self.t("Outbound", "出站")
        )
        .unwrap();
        if peak == 0 {
            write!(
                out,
                "<p class='empty'>{}</p>",
                self.t("No traffic recorded in this period", "此时段暂无流量记录")
            )
            .unwrap();
        } else {
            let mut points = [String::new(), String::new()];
            let width = history.history.len().max(1) * 16;
            let mut chart = format!(
                "<svg viewBox='0 0 {width} 60' aria-hidden='true' preserveAspectRatio='none'>"
            );
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
            if let (Some(first), Some(last)) = (history.history.first(), history.history.last()) {
                write!(
                    out,
                    "<div class='axis'><span>{}</span><span>{}</span></div>",
                    escape(&first.date),
                    escape(&last.date)
                )
                .unwrap();
            }
        }
        write!(
            out,
            "<details><summary>{}</summary>",
            self.t("Traffic by period", "分时流量明细")
        )
        .unwrap();
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
        out + "</tbody></table></div></details></section>"
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

fn traffic_size(bytes: u64) -> String {
    let mut value = bytes as f64;
    for unit in ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"] {
        if value < 1024.0 {
            return if unit == "B" {
                format!("{bytes} B")
            } else {
                format!("{value:.1} {unit}")
            };
        }
        value /= 1024.0;
    }
    unreachable!()
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
            "<!doctype html><html lang='{lang}' class='{theme}'><head><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'><title>{title} · Astrave</title><style>{}</style>",
            include_str!("dashboard.css")
        );
        if live {
            html.push_str("<meta http-equiv='refresh' content='5'>");
        }
        html.push_str("</head><body><header><div class='topbar'><a class='brand' href='/'><img src='/favicon.ico' width='30' height='30' alt=''>Astrave</a><nav>");
        for (url, detail, label) in [
            ("/", "/", v.t("Overview", "概览")),
            ("/clients", "/client", v.t("Clients", "客户端")),
            ("/tunnels", "/tunnel", v.t("Tunnels", "隧道")),
        ] {
            let current = if path == url {
                " aria-current='page'"
            } else if path == detail {
                " aria-current='location'"
            } else {
                ""
            };
            write!(html, "<a href='{url}'{current}>{label}</a>").unwrap();
        }
        write!(html, "</nav></div></header><main><h1>{title}</h1>").unwrap();
        match path {
            "/" => {
                let info = self.system_info().data;
                let s = info.status;
                let c = info.config;
                html.push_str("<dl class='metrics'>");
                for (label, value) in [
                    (
                        v.t("Clients online / total", "在线 / 全部客户端"),
                        format!("{} / {}", s.client_counts, s.total_client_counts),
                    ),
                    (
                        v.t("Active connections", "活动连接"),
                        s.active_connections.to_string(),
                    ),
                    (
                        v.t("Inbound today", "今日入站"),
                        traffic_size(s.total_traffic_in),
                    ),
                    (
                        v.t("Outbound today", "今日出站"),
                        traffic_size(s.total_traffic_out),
                    ),
                ] {
                    html.push_str("<div>");
                    field(&mut html, label, value);
                    html.push_str("</div>");
                }
                html.push_str("</dl>");
                write!(
                    html,
                    "<section class='resources'><h2>{}</h2><dl>",
                    v.t("Gateway process", "网关进程")
                )
                .unwrap();
                field(
                    &mut html,
                    v.t("Resident memory", "驻留内存"),
                    info.process
                        .resident_bytes
                        .map(traffic_size)
                        .unwrap_or_else(|| "—".into()),
                );
                field(
                    &mut html,
                    "CPU",
                    format!("{:.1}%", info.process.cpu_percent),
                );
                html.push_str("</dl></section>");
                html += &v.traffic(self.system_traffic(query).data);
                if !s.tunnel_type_count.is_empty() {
                    write!(
                        html,
                        "<section><h2>{}</h2><dl>",
                        v.t("Tunnels by protocol", "各协议隧道数")
                    )
                    .unwrap();
                    for (ty, n) in s.tunnel_type_count {
                        field(&mut html, &ty.to_ascii_uppercase(), n);
                    }
                    html.push_str("</dl></section>");
                }
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
            "<footer><nav>{} {} {} {}</nav></footer></main></body></html>",
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
