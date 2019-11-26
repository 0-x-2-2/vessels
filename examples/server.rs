use vessels::{
    channel::IdChannel,
    core,
    core::{hal::network::Server, Executor},
    format::Cbor,
    replicate::{collections::List, Share, Shared},
};

type Collection = Shared<dyn List<String>>;

pub fn main() {
    core::<Executor>().unwrap().run(async move {
        let collection = Collection::new(Box::new(vec![]));
        let mut server = Server::new().unwrap();
        server
            .listen::<Box<dyn List<String>>, IdChannel, Cbor>(
                "127.0.0.1:61200".parse().unwrap(),
                Box::new(move || {
                    let collection = collection.share();
                    Box::pin(async move { Box::new(collection) as Box<dyn List<String>> })
                }),
            )
            .await
            .unwrap();
    });
}
