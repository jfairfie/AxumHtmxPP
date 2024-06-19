mod template;
mod models;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicUsize;
use axum::extract::{Path, State};
use axum::Form;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::extract::WebSocketUpgrade;
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use crate::template::{HtmlTemplate, IndexTemplate, PointingButtonsTemplate, PointPageTemplate, PointTemplate, TodosTemplate};
use crate::models::{Msg, Msg1, Point, Todo, TodoForm, UserForm};

lazy_static! {
    static ref TODOS: Mutex<Vec<Todo>> = Mutex::new(Vec::new());
    static ref POINTS: Mutex<HashMap<usize, Point>> = Mutex::new(HashMap::new());
    static ref USER_UNBOUNDED_SENDERS: Mutex<HashMap<usize, UnboundedSender<Message>>> = Mutex::new(HashMap::new());
}

static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(1);
type Users = Arc<RwLock<HashSet<usize>>>;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let users = Users::default();

    let app = axum::Router::new()
        .route("/", get(index))
        .route("/todo", get(get_todos).post(create_todo))
        .route("/todo/:id", delete(delete_todo))
        .route("/point", get(pointpage))
        .route("/points", get(points))
        .route("/ws/points", get(ws_points_handler))
        .route("/test", post({ let users = users.clone(); move |form: Form<UserForm>| add_user(form, users) }))
        .with_state(users);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await.unwrap();
    println!("Listening on http://127.0.0.1:8080");

    axum::serve(listener, app).await.unwrap();

    Ok(())
}

async fn index() -> impl IntoResponse{ HtmlTemplate(IndexTemplate {}).into_response() }

async fn pointpage() -> impl IntoResponse {
    HtmlTemplate(PointPageTemplate {}).into_response()
}

async fn ws_points_handler(ws: WebSocketUpgrade, State(state): State<Users>) -> impl IntoResponse {
    ws.on_upgrade(|socket| points_websocket(socket, state))
}

async fn add_user(form: Form<UserForm>, state: Arc<RwLock<HashSet<usize>>>) -> impl IntoResponse {
    let my_id = NEXT_USER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    POINTS.lock().unwrap().insert(my_id, Point { point: 0.0, name: format!("{}", form.name) });
    state.write().await.insert(my_id);
    let points = POINTS.lock().unwrap().iter().map(|(usize, point)| point.clone()).collect();

    HtmlTemplate(PointingButtonsTemplate { name: form.name.clone(), id: my_id, points }).into_response()
}

async fn points_websocket(ws: WebSocket, state: Users) {
    println!("New connection!");

    let (mut sender, mut receiver) = ws.split();
    let (tx, mut rx): (UnboundedSender<Message>, UnboundedReceiver<Message>) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            println!("Received message {:?}", msg);
            sender.send(msg).await.expect("Error!");
        }
        sender.close().await.unwrap();
    });

    let mut my_id: Option<usize> = None;

    while let Some(Ok(result)) = receiver.next().await {
        if let Ok(result) = change_point(result) {
            my_id = Some(result);
            broadcast_point(&tx, result, &state).await;
        }
    }

    println!("Disconnecting");
    disconnect(my_id, &state).await;
}

async fn points() -> impl IntoResponse {
    let points: Vec<Point> = POINTS.lock().unwrap().iter().map(|(usize, point)| point.clone()).collect();
    HtmlTemplate(PointTemplate { points }).into_response()
}

async fn get_todos() -> impl IntoResponse {
    println!("Getting Todos");
    let todos = TODOS.lock().unwrap();
    let new_todos: Vec<Todo> = todos.iter().map(|todo| todo.clone()).collect();
    let template = TodosTemplate { todos: new_todos };
    HtmlTemplate(template)
}

async fn create_todo(form: Form<TodoForm>) -> impl IntoResponse {
    println!("Adding Form");
    let mut todos = TODOS.lock().unwrap();
    let id = todos.len() as u32 + 1;

    let todo = Todo { id: id as usize, text: form.text.clone() };
    todos.push(todo);
    let mut new_todos: Vec<Todo> = todos.iter_mut().map(|todo| todo.clone()).collect();

    HtmlTemplate(TodosTemplate { todos: new_todos })
}

async fn delete_todo(Path(id): Path<u32>) -> impl IntoResponse {
    println!("Deleting Todo");
    let mut todos = TODOS.lock().unwrap();
    let new_todos: Vec<Todo> = todos.iter().filter(|todo| todo.id != id as usize).map(|todo| todo.clone()).collect();
    HtmlTemplate(TodosTemplate { todos: new_todos })
}

async fn disconnect(my_id: Option<usize>, users: &Users) {
    if my_id.is_some() {
        users.write().await.remove(&my_id.unwrap());
        USER_UNBOUNDED_SENDERS.lock().unwrap().remove(&my_id.unwrap());
        POINTS.lock().unwrap().remove(&my_id.unwrap());
    }
}

async fn broadcast_point(tx: &UnboundedSender<Message>, my_id: usize, users: &Users) {
    let mut all_points: String = "".to_string();

    for point in POINTS.lock().unwrap().iter() {
        all_points.push_str(&format!("<li> {}: {:?}</li>", point.1.name, point.1.point).to_string());
    }

    if !USER_UNBOUNDED_SENDERS.lock().unwrap().contains_key(&my_id) {
        USER_UNBOUNDED_SENDERS.lock().unwrap().insert(my_id, tx.clone());
    }

    println!("USER_UNBOUNDED: {:?}", USER_UNBOUNDED_SENDERS.lock().unwrap());

    let tst_message = format!("<ul id='point'> {} </ul>", all_points).to_string();

    let mut disconnect_ids: Vec<usize> = vec![];

    for (&_uid, tx) in USER_UNBOUNDED_SENDERS.lock().unwrap().iter() {
        match tx.send(Message::Text(tst_message.clone())) {
            Ok(_) => { println!("Sent"); },
            Err(err) => {
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

async fn broadcast_msg(msg: Message, users: &Users) {
    if let Message::Text(msg) = msg {
        for (uid, tx) in USER_UNBOUNDED_SENDERS.lock().unwrap().iter() {
            tx.send(Message::Text(msg.clone())).expect("Failed to send message!");
        }
    }
}

fn change_point(result: Message) -> Result<usize, String> {
    println!("Changing point");
    match result {
        Message::Text(msg) => {
            println!("Msg {:?}", msg);
            let mut msg: Msg1 = serde_json::from_str(&msg).expect("Failed to parse message");
            let my_id = msg.id.parse::<usize>().unwrap();

            match msg.point.parse::<f32>() {
                Ok(point) => POINTS.lock().unwrap().get_mut(&my_id).unwrap().point = point,
                Err(_) => return Err("Invalid point value".to_string())
            }

            POINTS.lock().unwrap().get_mut(&my_id).unwrap().point = msg.point.parse::<f32>().unwrap();
            println!("Point changed to {:?}", POINTS.lock().unwrap().get(&my_id).unwrap().point);
            Ok(my_id)
        },
        _ => Err("Invalid message".to_string())
    }
}

fn enrich_result(result: Message, id: usize) -> Result<Message, serde_json::Error> {
    println!("Enriching {:?}", result);
    match result {
        Message::Text(msg) => {
            let mut msg1: Msg1 = serde_json::from_str(&msg)?;
            let mut msg: Msg = serde_json::from_str(&msg)?;
            msg.uid = Some(id);
            let msg = serde_json::to_string(&msg)?;
            Ok(Message::Text(msg))
        }
        _ => Ok(result)
    }
}