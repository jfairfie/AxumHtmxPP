use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicUsize;

use axum::extract::State;
use axum::extract::WebSocketUpgrade;
use axum::extract::ws::{Message, WebSocket};
use axum::response::IntoResponse;
use axum::routing::{get};
use futures::{SinkExt, StreamExt};
use lazy_static::lazy_static;
use tokio::sync::{mpsc, RwLock};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::models::{WSMessage, Point};
use crate::template::{HtmlTemplate, PointingPageTemplate};

mod template;
mod models;

lazy_static! {
    static ref POINTS: Mutex<HashMap<usize, Point>> = Mutex::new(HashMap::new());
    static ref USER_UNBOUNDED_SENDERS: Mutex<HashMap<usize, UnboundedSender<Message>>> = Mutex::new(HashMap::new());
    static ref ROOM_USER_UNBOUNDED_SENDERS: Mutex<HashMap<usize, HashMap<usize, UnboundedSender<Message>>>> = Mutex::new(HashMap::new());
    static ref BOARD_SHOWN: Mutex<bool> = Mutex::new(false);
}

static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(1);
type Users = Arc<RwLock<HashSet<usize>>>;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let users = Users::default();

    let app = axum::Router::new()
        .route("/", get(points_page))
        .route("/ws/points", get(ws_points_handler))
        .with_state(users);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("Listening on http://0.0.0.0:8080");

    axum::serve(listener, app).await.unwrap();

    Ok(())
}

async fn points_page() -> impl IntoResponse {
    HtmlTemplate(PointingPageTemplate { id: "".to_string(), point: "".to_string() }).into_response()
}

async fn ws_points_handler(ws: WebSocketUpgrade, State(state): State<Users>) -> impl IntoResponse {
    ws.on_upgrade(|socket| points_websocket(socket, state))
}

async fn points_websocket(ws: WebSocket, state: Users) {
    println!("New connection!");

    let (mut sender, mut receiver) = ws.split();
    let (tx, mut rx): (UnboundedSender<Message>, UnboundedReceiver<Message>) = mpsc::unbounded_channel();
    let my_id = NEXT_USER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await { sender.send(msg).await.expect("Error!"); }
        sender.close().await.unwrap();
    });

    while let Some(Ok(result)) = receiver.next().await {
        if let Ok(_) = consume_message(my_id, result, &tx) {
            broadcast_point(&state).await;
        }
    }

    println!("Disconnecting");
    disconnect(Some(my_id), &state).await;
    broadcast_point(&state).await;
}

async fn disconnect(my_id: Option<usize>, users: &Users) {
    if my_id.is_some() {
        users.write().await.remove(&my_id.unwrap());
        USER_UNBOUNDED_SENDERS.lock().unwrap().remove(&my_id.unwrap());
        POINTS.lock().unwrap().remove(&my_id.unwrap());
    }
}



async fn broadcast_point(users: &Users) {
    let mut all_points: String = "".to_string();

    let show = BOARD_SHOWN.lock().unwrap().clone();

    let new_show: String;
    let btn_text: String;
    let shown_css: String;

    let cleared = POINTS.lock().unwrap().iter().all(|(_, point)| point.point == 0.0);

    match show && !cleared {
        false => {
            new_show = "true".to_string();
            btn_text = "Show".to_string();
            shown_css = "hidden".to_string();
        },
        true => {
            new_show = "false".to_string();
            btn_text = "Hide".to_string();
            shown_css = "".to_string();
        }
    }

    for point in POINTS.lock().unwrap().iter() {
        println!("{}", point.1.point);
        if point.1.point as i32 == 0 {
            all_points.push_str(&format!("<li> {}: <span class=\"{}\"> {:?} </span></li>", point.1.name, shown_css, point.1.point).to_string());
        } else {
            all_points.push_str(&format!("<li> <div class=\"check\"></div> {}: <span class=\"{}\"> {:?} </span></li>", point.1.name, shown_css, point.1.point).to_string());
        }
    }

    let show_hide: String = format!("<button type=\"button\" id=\"showbtn\" hx-vals='{{\"show\": \"{}\"}}' ws-send name=\"show\">{}</button>", new_show, btn_text).to_string();

    let tst_message = format!("\
        <form id='newform' ws-send>
            <input name=\"point\" required readonly id=\"numsubmit\" hidden/>
            <button type=\"submit\" onclick=\"document.getElementById(\'numsubmit\').value = \'.5\'\"> .5 </button>
            <button type=\"submit\" onclick=\"document.getElementById(\'numsubmit\').value = \'1\'\"> 1 </button>
            <button type=\"submit\" onclick=\"document.getElementById(\'numsubmit\').value = \'2\'\"> 2 </button>
            <button type=\"submit\" onclick=\"document.getElementById(\'numsubmit\').value = \'3\'\"> 3 </button>
            <button type=\"submit\" onclick=\"document.getElementById(\'numsubmit\').value = \'5\'\"> 5 </button>
            <button type=\"submit\" onclick=\"document.getElementById(\'numsubmit\').value = \'8\'\"> 8 </button>
        </form>
        {}
        <button type=\"button\" id=\"clearbtn\" ws-send hx-vals='{{\"clear\": \"true\"}}'>Clear All</button>
        <ul id='point'> {} </ul>\
    ", show_hide, all_points).to_string();

    let mut disconnect_ids: Vec<usize> = vec![];

    for (&_uid, tx) in USER_UNBOUNDED_SENDERS.lock().unwrap().iter() {
        match tx.send(Message::Text(tst_message.clone())) {
            Ok(_) => { println!("Sent"); },
            Err(_) => {
                disconnect_ids.push(_uid);
            }
        }

        tx.send(Message::Text(tst_message.clone())).unwrap();
    }

    if !disconnect_ids.is_empty() {
        for uid in disconnect_ids {
            disconnect(Some(uid), users).await;
        }
    }
}

fn consume_message(my_id: usize, result: Message, tx: &UnboundedSender<Message>) -> Result<(), String> {
    match result {
        Message::Text(msg) => {
            println!("{}", msg);
            let msg: WSMessage = serde_json::from_str(&msg).expect("Failed to parse message");

            // also need to add a new room

            // if this occurs then the user is being added
            if msg.id.is_some() && msg.id.clone().unwrap().len() == 0 {
                USER_UNBOUNDED_SENDERS.lock().unwrap().insert(my_id, tx.clone());
                POINTS.lock().unwrap().insert(my_id, Point { point: 0.0, name: format!("{}", msg.name.unwrap_or("".to_string())) });
            } else if msg.point.is_some() {
                match msg.point.unwrap().parse::<f32>() {
                    Ok(point) => POINTS.lock().unwrap().get_mut(&my_id).unwrap().point = point,
                    Err(_) => return Err("Invalid point value".to_string())
                }

                // POINTS.lock().unwrap().get_mut(&my_id).unwrap().point = msg.point.unwrap().parse::<f32>().unwrap();
                println!("Point changed to {:?}", POINTS.lock().unwrap().get(&my_id).unwrap().point);
            } else if msg.show.is_some() {
                println!("Show changed to {:?}", msg.show.clone());
                match msg.show.unwrap().as_str() {
                    "true" => BOARD_SHOWN.lock().unwrap().clone_from(&true),
                    "false" => BOARD_SHOWN.lock().unwrap().clone_from(&false),
                    _ => {}
                }
            } else if msg.clear.is_some() {
                println!("Clearing");

                for user in USER_UNBOUNDED_SENDERS.lock().unwrap().iter() {
                    POINTS.lock().unwrap().get_mut(&user.0).unwrap().point = 0.0;
                }
            }

            Ok(())
        },
        _ => Err("Invalid message".to_string())
    }
}