use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, Json},
    routing::get,
    Router,
};
use log::{info};
use std::collections::HashMap;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tokio::sync::RwLock;

// Simple state struct that we can create from the bot
#[derive(Clone)]
pub struct DashboardState {
    pub analytics_data: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    pub health_data: Arc<RwLock<HashMap<String, bool>>>,
    pub points_data: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    pub leaderboard_data: Arc<RwLock<Vec<serde_json::Value>>>,
}

impl DashboardState {
    pub fn new() -> Self {
        Self {
            analytics_data: Arc::new(RwLock::new(HashMap::new())),
            health_data: Arc::new(RwLock::new(HashMap::new())),
            points_data: Arc::new(RwLock::new(HashMap::new())),
            leaderboard_data: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn update_analytics(&self, data: HashMap<String, serde_json::Value>) {
        *self.analytics_data.write().await = data;
    }

    pub async fn update_health(&self, data: HashMap<String, bool>) {
        *self.health_data.write().await = data;
    }

    pub async fn update_points(&self, data: HashMap<String, serde_json::Value>) {
        *self.points_data.write().await = data;
    }

    pub async fn update_leaderboard(&self, data: Vec<serde_json::Value>) {
        *self.leaderboard_data.write().await = data;
    }
}

pub struct WebDashboard {
    state: DashboardState,
}

impl WebDashboard {
    pub fn new() -> Self {
        Self {
            state: DashboardState::new(),
        }
    }

    pub fn get_state(&self) -> DashboardState {
        self.state.clone()
    }

    pub async fn start_server(&self, port: u16) -> Result<(), Box<dyn std::error::Error>> {
        info!("Creating web server routes...");
        let app = self.create_routes();
        
        info!("Binding to 0.0.0.0:{}...", port);
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
        info!("Web dashboard available at http://localhost:{}", port);
        info!("Analytics API: http://localhost:{}/api/analytics", port);
        
        info!("Starting axum server...");
        axum::serve(listener, app).await?;
        Ok(())
    }

    fn create_routes(&self) -> Router {
        Router::new()
            // Main dashboard
            .route("/", get(dashboard_html))
            .route("/dashboard", get(dashboard_html))
            
            // API endpoints
            .route("/api/analytics", get(get_analytics))
            .route("/api/health", get(get_health))
            .route("/api/status", get(get_status))
            .route("/api/points", get(get_points_stats))
            .route("/api/leaderboard", get(get_leaderboard))
            
            // Enable CORS for API endpoints
            .layer(CorsLayer::permissive())
            .with_state(self.state.clone())
    }
}

// API Route Handlers

async fn get_analytics(State(state): State<DashboardState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let analytics = state.analytics_data.read().await.clone();
    Ok(Json(serde_json::json!({
        "success": true,
        "data": analytics
    })))
}

async fn get_health(State(state): State<DashboardState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let health = state.health_data.read().await.clone();
    Ok(Json(serde_json::json!({
        "success": true,
        "data": health
    })))
}

async fn get_status(State(_state): State<DashboardState>) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({
        "success": true,
        "data": {
            "status": "running",
            "timestamp": chrono::Utc::now(),
            "version": env!("CARGO_PKG_VERSION")
        }
    })))
}

async fn get_points_stats(State(state): State<DashboardState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let points = state.points_data.read().await.clone();
    Ok(Json(serde_json::json!({
        "success": true,
        "data": points
    })))
}

async fn get_leaderboard(State(state): State<DashboardState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let leaderboard = state.leaderboard_data.read().await.clone();
    Ok(Json(serde_json::json!({
        "success": true,
        "data": leaderboard
    })))
}

// Embedded HTML Dashboard
async fn dashboard_html() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

const DASHBOARD_HTML: &str = r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>NotABot Dashboard</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
            font-family: 'Segoe UI', system-ui, -apple-system, sans-serif;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: #333;
            min-height: 100vh;
            padding: 20px;
        }
        .container {
            max-width: 1200px;
            margin: 0 auto;
            background: white;
            border-radius: 20px;
            box-shadow: 0 25px 50px rgba(0,0,0,0.15);
            overflow: hidden;
        }
        .header {
            background: linear-gradient(135deg, #4facfe 0%, #00f2fe 100%);
            color: white;
            padding: 40px;
            text-align: center;
            position: relative;
        }
        .header h1 {
            font-size: 3rem;
            font-weight: 700;
            margin-bottom: 10px;
        }
        .header p {
            opacity: 0.9;
            font-size: 1.2rem;
        }
        .refresh-btn {
            background: rgba(255,255,255,0.2);
            color: white;
            border: 2px solid rgba(255,255,255,0.3);
            padding: 12px 24px;
            border-radius: 50px;
            font-size: 1rem;
            cursor: pointer;
            transition: all 0.3s;
            margin-top: 20px;
        }
        .refresh-btn:hover {
            background: rgba(255,255,255,0.3);
            transform: translateY(-2px);
        }
        .stats-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
            gap: 25px;
            padding: 40px;
        }
        .stat-card {
            background: linear-gradient(135deg, #f8f9fa 0%, #e9ecef 100%);
            border-radius: 16px;
            padding: 30px;
            border-left: 5px solid #4facfe;
            transition: all 0.3s;
        }
        .stat-card:hover {
            transform: translateY(-5px);
            box-shadow: 0 15px 35px rgba(0,0,0,0.1);
        }
        .stat-value {
            font-size: 3rem;
            font-weight: 800;
            background: linear-gradient(135deg, #667eea, #764ba2);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            background-clip: text;
            margin-bottom: 8px;
        }
        .stat-label {
            color: #6c757d;
            font-size: 1rem;
            text-transform: uppercase;
            letter-spacing: 1px;
            font-weight: 600;
        }
        .section {
            margin: 20px 40px;
            padding: 30px;
            background: linear-gradient(135deg, #f8f9fa 0%, #e9ecef 100%);
            border-radius: 16px;
            border: 1px solid #dee2e6;
        }
        .section h2 {
            color: #2c3e50;
            margin-bottom: 20px;
            font-size: 1.8rem;
            font-weight: 600;
        }
        .loading {
            text-align: center;
            padding: 60px;
            color: #6c757d;
            font-size: 1.1rem;
        }
        .status-indicator {
            display: inline-block;
            width: 12px;
            height: 12px;
            border-radius: 50%;
            margin-right: 10px;
            box-shadow: 0 0 10px rgba(0,0,0,0.3);
        }
        .status-online { 
            background: linear-gradient(135deg, #27ae60, #2ecc71);
            box-shadow: 0 0 15px rgba(39, 174, 96, 0.5);
        }
        .status-offline { 
            background: linear-gradient(135deg, #e74c3c, #c0392b);
            box-shadow: 0 0 15px rgba(231, 76, 60, 0.5);
        }
        .connection-item {
            margin: 15px 0;
            padding: 20px;
            background: white;
            border-radius: 12px;
            border: 1px solid #e9ecef;
            transition: all 0.3s;
        }
        .connection-item:hover {
            transform: translateX(5px);
            box-shadow: 0 5px 15px rgba(0,0,0,0.1);
        }
        .error {
            color: #e74c3c;
            text-align: center;
            padding: 20px;
            background: #fadbd8;
            border-radius: 8px;
            margin: 10px 0;
        }
        @media (max-width: 768px) {
            .stats-grid { 
                grid-template-columns: 1fr; 
                padding: 20px;
            }
            .header { padding: 30px 20px; }
            .header h1 { font-size: 2.5rem; }
            .section { margin: 15px 20px; padding: 20px; }
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>NotaBot Dashboard</h1>
            <p>Real-time chat bot analytics and management</p>
            <button class="refresh-btn" onclick="loadAllData()">Refresh Data</button>
        </div>

        <div class="stats-grid" id="stats-grid">
            <div class="loading">Loading analytics...</div>
        </div>

        <div class="section">
            <h2>Platform Connections</h2>
            <div id="connections">
                <div class="loading">Checking connection status...</div>
            </div>
        </div>

        <div class="section">
            <h2>Bot Status</h2>
            <div id="status">
                <div class="loading">Loading bot status...</div>
            </div>
        </div>
    </div>

    <script>
        async function loadAllData() {
            console.log('Refreshing dashboard data...');
            await Promise.all([
                loadAnalytics(),
                loadConnections(),
                loadStatus()
            ]);
        }

        async function loadAnalytics() {
            try {
                const response = await fetch('/api/analytics');
                const result = await response.json();
                
                if (result.success) {
                    const data = result.data;
                    document.getElementById('stats-grid').innerHTML = `
                        <div class="stat-card">
                            <div class="stat-value">${formatNumber(data.total_users || 0)}</div>
                            <div class="stat-label">Total Users</div>
                        </div>
                        <div class="stat-card">
                            <div class="stat-value">${formatNumber(data.total_messages || 0)}</div>
                            <div class="stat-label">Messages Processed</div>
                        </div>
                        <div class="stat-card">
                            <div class="stat-value">${formatNumber(data.total_commands_used || 0)}</div>
                            <div class="stat-label">Commands Executed</div>
                        </div>
                        <div class="stat-card">
                            <div class="stat-value">${formatNumber(data.total_spam_blocked || 0)}</div>
                            <div class="stat-label">Spam Blocked</div>
                        </div>
                        <div class="stat-card">
                            <div class="stat-value">${formatNumber(data.regular_users || 0)}</div>
                            <div class="stat-label">Regular Users</div>
                        </div>
                        <div class="stat-card">
                            <div class="stat-value">${data.uptime_hours || 0}h</div>
                            <div class="stat-label">Bot Uptime</div>
                        </div>
                    `;
                    console.log('Analytics loaded successfully');
                } else {
                    document.getElementById('stats-grid').innerHTML = '<div class="loading">No analytics data available yet</div>';
                }
            } catch (error) {
                console.error('❌ Failed to load analytics:', error);
                document.getElementById('stats-grid').innerHTML = 
                    '<div class="error">❌ Failed to load analytics data</div>';
            }
        }

        async function loadConnections() {
            try {
                const response = await fetch('/api/health');
                const result = await response.json();
                
                if (result.success) {
                    const connections = Object.entries(result.data)
                        .map(([platform, status]) => `
                            <div class="connection-item">
                                <span class="status-indicator ${status ? 'status-online' : 'status-offline'}"></span>
                                <strong>${platform.toUpperCase()}</strong>
                                <span style="float: right; color: ${status ? '#27ae60' : '#e74c3c'}; font-weight: 600;">
                                    ${status ? 'Connected' : 'Disconnected'}
                                </span>
                            </div>
                        `).join('');
                    
                    document.getElementById('connections').innerHTML = 
                        connections || '<div class="connection-item">No platforms configured</div>';
                    console.log('Connection status loaded');
                } else {
                    document.getElementById('connections').innerHTML = '<div class="connection-item">No connection data available</div>';
                }
            } catch (error) {
                console.error('❌ Failed to load connections:', error);
                document.getElementById('connections').innerHTML = 
                    '<div class="error">❌ Failed to load connection status</div>';
            }
        }

        async function loadStatus() {
            try {
                const response = await fetch('/api/status');
                const result = await response.json();
                
                if (result.success) {
                    const data = result.data;
                    const uptime = new Date(data.timestamp).toLocaleString();
                    
                    document.getElementById('status').innerHTML = `
                        <div class="connection-item">
                            <span class="status-indicator status-online"></span>
                            <strong>Bot Status:</strong> ${data.status}
                            <span style="float: right; color: #6c757d;">
                                Version: ${data.version}
                            </span>
                        </div>
                        <div class="connection-item">
                            <span class="status-indicator status-online"></span>
                            <strong>Last Update:</strong> ${uptime}
                        </div>
                    `;
                    console.log('Status loaded');
                }
            } catch (error) {
                console.error('❌ Failed to load status:', error);
                document.getElementById('status').innerHTML = 
                    '<div class="error">❌ Failed to load status</div>';
            }
        }

        function formatNumber(num) {
            if (num >= 1000000) return (num / 1000000).toFixed(1) + 'M';
            if (num >= 1000) return (num / 1000).toFixed(1) + 'K';
            return num.toString();
        }

        // Initialize dashboard
        document.addEventListener('DOMContentLoaded', () => {
            console.log('NotaBot Dashboard initialized');
            loadAllData();
            
            // Auto-refresh every 30 seconds
            setInterval(loadAllData, 30000);
        });
    </script>
</body>
</html>
"#;