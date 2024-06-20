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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WSMessage {
    pub name: Option<String>,
    pub point: Option<String>,
    pub id: Option<String>,
    pub show: Option<String>,
    pub clear: Option<String>,
}