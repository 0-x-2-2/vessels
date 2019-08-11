use futures::Future as Fut;
use futures::{lazy, IntoFuture, Stream};
use vessels::{
    executor,
    protocol::{self, protocol, Future, Protocol, Remote, Value},
};

#[derive(Value, Debug)]
pub enum TestEnum {
    Numbers(u64, u32),
    Text(String),
    Empty,
}

#[derive(Value)]
pub struct TestStruct {
    data: String,
    number: u64,
    future: Future<TestEnum, ()>,
}

#[protocol]
pub trait TestProtocol {
    fn test(&self, number: u64) -> Future<TestStruct, ()>;
    fn sec_test(&self);
}

struct Test;

impl TestProtocol for Test {
    fn test(&self, number: u64) -> Future<TestStruct, ()> {
        println!("test");
        protocol::Future::new(
            Ok(TestStruct {
                data: "test".to_owned(),
                number,
                future: Future::new(Ok(TestEnum::Numbers(number, 25)).into_future()),
            })
            .into_future(),
        )
    }
    fn sec_test(&self) {
        println!("sec_test");
    }
}

fn main() {
    let rem = TestProtocol::remote();
    let (rem, rss) = rem.separate();
    let (rsink, rstream) = rss.split();
    let (sink, stream) = Test.into_protocol().split();
    executor::run(lazy(move || {
        executor::spawn(rstream.forward(sink).then(|_| Ok(())));
        executor::spawn(stream.forward(rsink).then(|_| Ok(())));
        println!("{:?}", rem.test(52).wait().unwrap().future.wait());
        Ok(())
    }));
}
