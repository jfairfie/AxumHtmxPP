use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Todo {
    pub id: usize,
    pub text: String
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TodoForm {
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserForm {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Point {
    pub point: f32,
    pub name: String,
}


#[derive(Serialize, Deserialize)]
pub struct Msg {
    pub name: String,
    pub uid: Option<usize>,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Msg1 {
    pub point: String,
    pub id: String,
}