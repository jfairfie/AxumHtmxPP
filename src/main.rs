use std::collections::{HashMap};
use std::sync::{Mutex};
use std::sync::atomic::AtomicUsize;

use axum::extract::{Path};
use axum::extract::WebSocketUpgrade;
use axum::extract::ws::{Message, WebSocket};
use axum::Form;
use axum::response::IntoResponse;
use axum::routing::{delete, get};
use futures::{SinkExt, StreamExt};
use lazy_static::lazy_static;
use tokio::sync::{mpsc};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::models::{WSMessage, Point, Room, CreateRoomForm};
use crate::template::{CreateRoomTemplate, HtmlTemplate, PointingPageTemplate, RoomTemplate};

mod template;
mod models;

lazy_static! {
    static ref POINTS: Mutex<HashMap<usize, Point>> = Mutex::new(HashMap::new());
    static ref USER_UNBOUNDED_SENDERS: Mutex<HashMap<usize, UnboundedSender<Message>>> = Mutex::new(HashMap::new());
    static ref ROOM_USER_UNBOUNDED_SENDERS: Mutex<HashMap<usize, HashMap<usize, UnboundedSender<Message>>>> = Mutex::new(HashMap::new());
    static ref ROOMS: Mutex<HashMap<usize, Room>> = Mutex::new(HashMap::new());
    static ref USER_ROOM: Mutex<HashMap<usize, usize>> = Mutex::new(HashMap::new());
}

static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(1);
static NEXT_ROOM_ID: AtomicUsize = AtomicUsize::new(1);

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let app = axum::Router::new()
        .route("/", get(room_page).post(create_room))
        .route("/rooms", get(get_rooms))
        .route("/rooms/:id", get(points_page))
        .route("/ws/points", get(ws_points_handler))
        .route("/delete_room/:id", delete(delete_room));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("Listening on http://0.0.0.0:8080");

    axum::serve(listener, app).await.unwrap();

    Ok(())
}

async fn room_page() -> impl IntoResponse {
    HtmlTemplate(CreateRoomTemplate { }).into_response()
}

async fn get_rooms() -> impl IntoResponse {
    let rooms = ROOMS.lock().unwrap().iter().map(|(_, room)| room.clone()).collect();
    HtmlTemplate(RoomTemplate { rooms }).into_response()
}

async fn create_room(form: Form<CreateRoomForm>) -> impl IntoResponse {
    println!("Creating room");
    let room_number = NEXT_ROOM_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let room = Room { room_id: room_number,name: form.name.clone(), board_shown: false };
    ROOMS.lock().unwrap().insert(room_number, room);
    ROOM_USER_UNBOUNDED_SENDERS.lock().unwrap().insert(room_number, HashMap::new());
    let rooms = ROOMS.lock().unwrap().iter().map(|(_, room)| room.clone()).collect();

    HtmlTemplate(RoomTemplate { rooms }).into_response()
}

async fn points_page(Path(id): Path<u32>) -> impl IntoResponse {

    if ROOMS.lock().unwrap().get(&(id as usize)).is_some() {
        return HtmlTemplate(PointingPageTemplate { id: "".to_string(), point: "".to_string(), room_id: id }).into_response()
    }


    let rooms: Vec<Room> = ROOMS.lock().unwrap().iter().map(|(_, room)| room.clone()).collect();
    return HtmlTemplate(RoomTemplate { rooms }).into_response();
}

async fn delete_room(Path(id): Path<u32>) -> impl IntoResponse {
    ROOMS.lock().unwrap().remove(&(id as usize));
    ROOM_USER_UNBOUNDED_SENDERS.lock().unwrap().remove(&(id as usize));

    let rooms: Vec<Room> = ROOMS.lock().unwrap().iter().map(|(_, room)| room.clone()).collect();
    HtmlTemplate(RoomTemplate { rooms }).into_response()
}

async fn ws_points_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(|socket| points_websocket(socket))
}

async fn points_websocket(ws: WebSocket) {
    println!("New connection!");

    let (mut sender, mut receiver) = ws.split();
    let (tx, mut rx): (UnboundedSender<Message>, UnboundedReceiver<Message>) = mpsc::unbounded_channel();
    let my_id = NEXT_USER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await { sender.send(msg).await.expect("Error!"); }
        sender.close().await.unwrap();
    });

    while let Some(Ok(result)) = receiver.next().await {
        if let Ok(room_id) = consume_message(my_id, result, &tx) {
            broadcast_point(room_id).await;
        }
    }

    let room_id = USER_ROOM.lock().unwrap().get(&my_id).unwrap().clone();
    disconnect(Some(my_id)).await;
    broadcast_point(room_id).await;
}

async fn disconnect(my_id: Option<usize>) {
    if my_id.is_some() {
        POINTS.lock().unwrap().remove(&my_id.unwrap());
        ROOM_USER_UNBOUNDED_SENDERS.lock().unwrap().get_mut(&USER_ROOM.lock().unwrap().get(&my_id.unwrap()).unwrap()).unwrap().remove(&my_id.unwrap());
    }
}

async fn broadcast_point(room_id: usize) {
    let mut all_points: String = "".to_string();
    let show = ROOMS.lock().unwrap().get(&room_id).unwrap().board_shown;

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

    let user_ids_on_room: Vec<usize> = ROOM_USER_UNBOUNDED_SENDERS.lock().unwrap().get(&room_id).unwrap().keys().cloned().collect();
    for point in POINTS.lock().unwrap().iter() {
        if user_ids_on_room.contains(&point.0) {
            if point.1.point as i32 == 0 {
                all_points.push_str(&format!("<li> {}: <span class=\"{}\"> {:?} </span></li>", point.1.name, shown_css, point.1.point).to_string());
            } else {
                all_points.push_str(&format!("<li> <div class=\"check\"></div> {}: <span class=\"{}\"> {:?} </span></li>", point.1.name, shown_css, point.1.point).to_string());
            }
        }
    }

    let show_hide: String = format!("<button type=\"button\" id=\"showbtn\" hx-vals='{{\"show\": \"{}\", \"room_id\": \"{}\"}}' ws-send name=\"show\">{}</button>", new_show, room_id, btn_text).to_string();

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
        <button type=\"button\" id=\"clearbtn\" ws-send hx-vals='{{\"clear\": \"true\", \"room_id\": \"{}\"}}'>Clear All</button>
        <ul id='point'> {} </ul>\
    ", show_hide, room_id, all_points).to_string();

    let mut disconnect_ids: Vec<usize> = vec![];

    for (&_uid, tx) in ROOM_USER_UNBOUNDED_SENDERS.lock().unwrap().get(&room_id).unwrap().iter() {
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
            disconnect(Some(uid)).await;
        }
    }
}

fn consume_message(my_id: usize, result: Message, tx: &UnboundedSender<Message>) -> Result<usize, String> {
    match result {
        Message::Text(msg) => {
            let mut room_id: Option<usize> = None;
            let msg: WSMessage = serde_json::from_str(&msg).expect("Failed to parse message");
            println!("{:?}", msg);

            match USER_ROOM.lock().unwrap().get(&my_id).clone() {
                Some(result) => { room_id = Some(result.clone()); },
                None => {}
            }

            // if this occurs then the user is being added
            if msg.id.is_some() && msg.id.clone().unwrap().len() == 0 && msg.room_id.is_some() {
                println!("Inserting room, user");
                room_id = Some(msg.room_id.unwrap().parse::<usize>().unwrap());

                ROOM_USER_UNBOUNDED_SENDERS.lock().unwrap().get_mut(&room_id.unwrap()).unwrap().insert(my_id, tx.clone());

                match USER_ROOM.lock() {
                    Ok(mut user_room) => { user_room.insert(my_id, room_id.unwrap().clone()); }
                    Err(err) => { println!("Error locking/inserting into user room: {:?}", err); }
                }

                POINTS.lock().unwrap().insert(my_id, Point { point: 0.0, name: format!("{}", msg.name.unwrap_or("".to_string())) });
                println!("Completed room, user insertion");
            } else if msg.point.is_some() {
                match msg.point.unwrap().parse::<f32>() {
                    Ok(point) => POINTS.lock().unwrap().get_mut(&my_id).unwrap().point = point,
                    Err(_) => return Err("Invalid point value".to_string())
                }
            } else if msg.show.is_some() {
                match msg.show.unwrap().as_str() {
                    "true" => ROOMS.lock().unwrap().get_mut(&msg.room_id.unwrap().parse::<usize>().unwrap()).unwrap().board_shown = true,
                    "false" => ROOMS.lock().unwrap().get_mut(&msg.room_id.unwrap().parse::<usize>().unwrap()).unwrap().board_shown = false,
                    _ => {}
                }
            } else if msg.clear.is_some() {
                for user in ROOM_USER_UNBOUNDED_SENDERS.lock().unwrap().get(&USER_ROOM.lock().unwrap().get(&my_id).unwrap()).unwrap().iter() {
                    POINTS.lock().unwrap().get_mut(&user.0).unwrap().point = 0.0;
                }
            }

            Ok(room_id.unwrap().to_owned())
        },
        _ => Err("Invalid message".to_string())
    }
}