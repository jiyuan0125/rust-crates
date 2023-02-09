use std::sync::Arc;

use axum::{
    extract::{ws::Message, WebSocketUpgrade},
    response::IntoResponse,
    Extension,
};
use dashmap::{DashMap, DashSet};
use futures::prelude::*;
pub use ws_shared::{Msg, MsgData};
use tokio::sync::broadcast;
use tracing::log::warn;

const CAPACITY: usize = 64;

#[derive(Debug)]
struct State {
    // for a given user, how many rooms they are in
    user_rooms: DashMap<String, DashSet<String>>,
    // for a given room, how many users are in it
    room_users: DashMap<String, DashSet<String>>,
    tx: broadcast::Sender<Arc<Msg>>,
}

#[derive(Clone, Debug, Default)]
pub struct ChatState(Arc<State>);

impl Default for State {
    fn default() -> Self {
        let (tx, _) = broadcast::channel(CAPACITY);
        Self {
            user_rooms: Default::default(),
            room_users: Default::default(),
            tx,
        }
    }
}

impl ChatState {
    pub fn get_user_rooms(&self, username: &str) -> Vec<String> {
        self.0
            .user_rooms
            .get(username)
            .map(|rooms| rooms.clone().into_iter().collect())
            .unwrap_or_default()
    }

    pub fn get_room_users(&self, room: &str) -> Vec<String> {
        self.0
            .room_users
            .get(room)
            .map(|users| users.clone().into_iter().collect())
            .unwrap_or_default()
    }
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Extension(state): Extension<ChatState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket<S>(socket: S, state: ChatState)
where
    S: Stream<Item = Result<Message, axum::Error>>
        + Sink<Message, Error = axum::Error>
        + Send
        + 'static,
{
    let mut rs = state.0.tx.subscribe();
    let cloned_state = state.clone();
    let (mut sender, mut receiver) = socket.split();

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(data)) = receiver.next().await {
            match data {
                Message::Text(msg) => {
                    let msg = Msg::try_from(msg.as_str()).unwrap();
                    handle_message(msg, cloned_state.0.clone()).await;
                }
                _ => {}
            }
        }
    });

    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rs.recv().await {
            let data = msg.as_ref().try_into().unwrap();
            if let Err(e) = sender.send(Message::Text(data)).await {
                warn!("websocket send error: {}", e);
                break;
            }
        }
    });

    // if any of the tasks fail, we want to shutdown the other one
    tokio::select! {
        _ = &mut recv_task => send_task.abort(),
        _ = &mut send_task => recv_task.abort(),
    }

    warn!("websocket closed");
    // this user has disconnected, so we need to remove the user from all rooms
    // usually we can get username from header, here we just use "fake_user"
    let username = "fake_user";
    for room in state.get_user_rooms(username) {
        let msg = Msg::leave(room.as_str(), username);
        let _ = state.0.tx.send(Arc::new(msg)).unwrap();
    }
}

async fn handle_message(msg: Msg, state: Arc<State>) {
    match msg.data {
        MsgData::Join => {
            state
                .user_rooms
                .entry(msg.username.clone())
                .or_default()
                .insert(msg.room.clone());
            state
                .room_users
                .entry(msg.room.clone())
                .or_default()
                .insert(msg.username.clone());
        }
        MsgData::Leave => {
            if let Some(room) = state.user_rooms.get_mut(&msg.username) {
                room.remove(&msg.room);
                if room.is_empty() {
                    drop(room);
                    state.user_rooms.remove(&msg.username);
                }
            }
            if let Some(users) = state.room_users.get_mut(&msg.room) {
                users.remove(&msg.username);
                if users.is_empty() {
                    drop(users);
                    state.room_users.remove(&msg.room);
                }
            }
        }
        _ => {}
    }

    let _ = state.tx.send(Arc::new(msg));
}

#[cfg(test)]
mod tests {

    use crate::test_tools::FakeClient;

    use super::*;
    use anyhow::Result;
    use assert_unordered::assert_eq_unordered;

    async fn prepare_connection() -> Result<(FakeClient<Message>, FakeClient<Message>, ChatState)> {
        let (client1, socket1) = test_tools::create_fake_connection();
        let (client2, socket2) = test_tools::create_fake_connection();
        let state = ChatState::default();

        let cloned_state = state.clone();
        tokio::spawn(async move {
            handle_socket(socket1, cloned_state).await;
        });

        let cloned_state = state.clone();
        tokio::spawn(async move {
            handle_socket(socket2, cloned_state).await;
        });

        Ok((client1, client2, state))
    }

    #[tokio::test]
    async fn handle_join_works() -> Result<()> {
        let (mut client1, mut client2, state) = prepare_connection().await?;

        let msg1 = &Msg::join("room1", "user1");
        let _ = client1.send(Message::Text(msg1.try_into()?)).await?;

        let msg2 = &Msg::join("room1", "user2");
        let _ = client2.send(Message::Text(msg2.try_into()?)).await?;

        let _ = verify(&mut client1, "room1", "user1", MsgData::Join).await;
        let _ = verify(&mut client1, "room1", "user2", MsgData::Join).await;

        let _ = verify(&mut client2, "room1", "user1", MsgData::Join).await;
        let _ = verify(&mut client2, "room1", "user2", MsgData::Join).await;

        assert_eq_unordered!(state.get_user_rooms("user1"), vec!["room1".into()]);
        assert_eq_unordered!(state.get_user_rooms("user2"), vec!["room1".into()]);
        assert_eq_unordered!(
            state.get_room_users("room1"),
            vec!["user1".into(), "user2".into()]
        );

        Ok(())
    }

    #[tokio::test]
    async fn handle_message_works() -> Result<()> {
        let (mut client1, mut client2, _state) = prepare_connection().await?;

        let msg1 = &Msg::join("room1", "user1");
        let _ = client1.send(Message::Text(msg1.try_into()?)).await?;

        let msg2 = &Msg::message("room1", "user1", "hello");
        let _ = client1.send(Message::Text(msg2.try_into()?)).await?;

        let _ = verify(&mut client1, "room1", "user1", MsgData::Join).await;
        let _ = verify(
            &mut client1,
            "room1",
            "user1",
            MsgData::Message("hello".into()),
        )
        .await;

        let _ = verify(&mut client2, "room1", "user1", MsgData::Join).await;
        let _ = verify(
            &mut client2,
            "room1",
            "user1",
            MsgData::Message("hello".into()),
        )
        .await;

        Ok(())
    }

    #[tokio::test]
    async fn handle_leave_works() -> Result<()> {
        let (mut client1, mut client2, _state) = prepare_connection().await?;

        let msg1 = &Msg::join("room1", "user1");
        let _ = client1.send(Message::Text(msg1.try_into()?)).await?;

        let msg2 = &Msg::leave("room1", "user1");
        let _ = client1.send(Message::Text(msg2.try_into()?)).await?;

        let _ = verify(&mut client1, "room1", "user1", MsgData::Join).await;
        let _ = verify(&mut client1, "room1", "user1", MsgData::Leave).await;

        let _ = verify(&mut client2, "room1", "user1", MsgData::Join).await;
        let _ = verify(&mut client2, "room1", "user1", MsgData::Leave).await;

        Ok(())
    }

    async fn verify<S>(client: &mut S, room: &str, username: &str, data: MsgData) -> Result<()>
    where
        S: Stream<Item = Message> + Unpin,
    {
        if let Some(Message::Text(msg1)) = client.next().await {
            let msg = Msg::try_from(msg1.as_str())?;
            assert_eq!(msg.room, room);
            assert_eq!(msg.username, username);
            assert_eq!(msg.data, data);
        }

        Ok(())
    }
}

mod test_tools {
    use std::marker::PhantomData;
    use std::task::Poll;

    use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
    use futures::prelude::*;
    use pin_project::pin_project;

    #[pin_project]
    pub struct FakeSocket<T, E> {
        #[pin]
        sink: FakeSink<T, E>,
        #[pin]
        stream: FakeStream<T, E>,
        marker: PhantomData<E>,
    }

    impl<T, E> FakeSocket<T, E> {
        pub fn new(sender: UnboundedSender<T>, receiver: UnboundedReceiver<T>) -> Self {
            Self {
                sink: FakeSink::new(sender),
                stream: FakeStream::new(receiver),
                marker: PhantomData,
            }
        }
    }

    #[pin_project]
    pub struct FakeSink<T, E> {
        #[pin]
        inner: UnboundedSender<T>,
        maker: PhantomData<E>,
    }

    impl<T, E> FakeSink<T, E> {
        pub fn new(sender: UnboundedSender<T>) -> Self {
            Self {
                inner: sender,
                maker: PhantomData,
            }
        }
    }

    #[pin_project]
    struct FakeStream<T, E> {
        #[pin]
        inner: UnboundedReceiver<T>,
        maker: PhantomData<E>,
    }

    impl<T, E> FakeStream<T, E> {
        pub fn new(receiver: UnboundedReceiver<T>) -> Self {
            Self {
                inner: receiver,
                maker: PhantomData,
            }
        }
    }

    impl<T, E> Sink<T> for FakeSink<T, E> {
        type Error = E;

        fn poll_ready(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            let this = self.project();
            let _ = this.inner.poll_ready(cx);
            Poll::Ready(Ok(()))
        }

        fn start_send(self: std::pin::Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
            let this = self.project();
            let _ = this.inner.start_send(item);
            Ok(())
        }

        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            let this = self.project();
            let _ = this.inner.poll_flush(cx);
            Poll::Ready(Ok(()))
        }

        fn poll_close(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            let this = self.project();
            let _ = this.inner.poll_close(cx);
            Poll::Ready(Ok(()))
        }
    }

    impl<T, E> Sink<T> for FakeSocket<T, E> {
        type Error = E;

        fn poll_ready(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            let this = self.project();
            this.sink.poll_ready(cx)
        }

        fn start_send(self: std::pin::Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
            let this = self.project();
            this.sink.start_send(item)
        }

        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            let this = self.project();
            this.sink.poll_flush(cx)
        }

        fn poll_close(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            let this = self.project();
            this.sink.poll_close(cx)
        }
    }

    impl<T, E> Stream for FakeStream<T, E> {
        type Item = Result<T, E>;

        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            let this = self.project();
            let data = futures::ready!(this.inner.poll_next(cx));
            Poll::Ready(Ok(data).transpose())
        }
    }

    impl<T, E> Stream for FakeSocket<T, E> {
        type Item = Result<T, E>;

        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            let this = self.project();
            this.stream.poll_next(cx)
        }
    }

    #[pin_project]
    pub struct FakeClient<T> {
        #[pin]
        sender: UnboundedSender<T>,
        #[pin]
        receiver: UnboundedReceiver<T>,
    }

    impl<T> FakeClient<T> {
        fn new(sender: UnboundedSender<T>, receiver: UnboundedReceiver<T>) -> Self {
            Self { sender, receiver }
        }
    }

    impl<T> Sink<T> for FakeClient<T> {
        type Error = futures::channel::mpsc::SendError;

        fn poll_ready(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            let this = self.project();
            this.sender.poll_ready(cx)
        }

        fn start_send(self: std::pin::Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
            let this = self.project();
            this.sender.start_send(item)
        }

        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            let this = self.project();
            this.sender.poll_flush(cx)
        }

        fn poll_close(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            let this = self.project();
            this.sender.poll_close(cx)
        }
    }

    impl<T> Stream for FakeClient<T> {
        type Item = T;

        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            let this = self.project();
            this.receiver.poll_next(cx)
        }
    }

    pub fn create_fake_connection<T, E>() -> (FakeClient<T>, FakeSocket<T, E>) {
        let (sender1, receiver1) = futures::channel::mpsc::unbounded();
        let (sender2, receiver2) = futures::channel::mpsc::unbounded();

        let client = FakeClient::new(sender1, receiver2);
        let socket = FakeSocket::<T, E>::new(sender2, receiver1);

        (client, socket)
    }
}
