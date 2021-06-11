use std::error::Error;
use std::net::{SocketAddr};
use std::collections::HashMap;

use std::io::{self, Read, Write};

use mio::net::{TcpListener, TcpStream};
use mio::event::Event;
use mio::{Events, Interest, Poll, Token};

struct EchoServerHandler {}
const CHUNK_SIZE: usize = 1024;


impl EchoServerHandler {

    fn handle(connection: &mut EchoServerConnection, event: &Event) -> io::Result<bool> {
        if event.is_readable() {
            let mut connection_closed = false;
            let mut bytes_read = 0;
            let mut buf = vec![0; CHUNK_SIZE];

            loop {
                match connection.client_socket.read(&mut buf[bytes_read..]) {
                    Ok(0) => {
                        connection_closed = true;
                        break;
                    }
                    Ok(len) => {
                        bytes_read += len;
                        if bytes_read == buf.len() {
                            buf.resize(buf.len() + CHUNK_SIZE, 0);
                        }
                    }
                    Err(ref err) if would_block(err) => {
                        break
                    },
                    Err(err) => return Err(err)
                }
            }

            connection.buffer_to_echo = buf;
            connection.interest = Interest::WRITABLE;

            if connection_closed {
                println!("Connection closed");
                return Ok(true);
            }
        }

        if event.is_writable() {
            let mut buf: &[u8] = &connection.buffer_to_echo;
            let mut bytes_written = 0;

            loop {
                match connection.client_socket.write(buf) {
                    Ok(0) => {
                        connection.buffer_to_echo.clear();
                        break;
                    }
                    Ok(len) => {
                        bytes_written += len;
                        buf = &buf[bytes_written..];
                    }
                    Err(ref err) if would_block(err) => {},
                    Err(err) => return Err(err)
                }
            }
            connection.interest = Interest::READABLE;
        }

        Ok(false)
    }
}

struct EchoServerConnection {
    client_socket: TcpStream,
    buffer_to_echo: Vec<u8>,
    interest: Interest
}

impl EchoServerConnection {
    fn new(client_socket: TcpStream) -> Self {
        EchoServerConnection{
            client_socket,
            buffer_to_echo: Vec::new(),
            interest: Interest::READABLE
        }
    }
}

const SERVER: Token = Token(0);

struct EchoServer {
    server_socket: TcpListener,
    clients: HashMap<Token, EchoServerConnection>,
    token_counter: usize
}

impl EchoServer {
    fn new(addr: SocketAddr) -> Result<Self, Box<dyn Error>> {
        Ok(EchoServer {
            server_socket: TcpListener::bind(addr)?,
            token_counter: 1,
            clients: HashMap::new()
        })
    }

    fn run(&mut self) -> io::Result<()> {
        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(1024);

        poll.registry().register(&mut self.server_socket, SERVER, Interest::READABLE)?;

        loop {
            poll.poll(&mut events, None)?;
            for event in events.iter() {
                match event.token() {
                    SERVER => {
                        let (connection, address) = match self.server_socket.accept() {
                            Ok((connection, address)) => (connection, address),
                            Err(e) => {
                                return Err(e);
                            }
                        };

                        println!("Got connection #{} from address: {:?}", self.token_counter, address);
                        let current_token = Token(self.token_counter);
                        let mut client = EchoServerConnection::new(connection);
                        poll.registry().register(&mut client.client_socket, current_token, client.interest)?;

                        self.clients.insert(current_token, client);
                        self.token_counter += 1;
                    },
                    token => {
                        if let Some(connection) = self.clients.get_mut(&token) {
                            let done = EchoServerHandler::handle(connection, event)?;
                            if done {
                                self.clients.remove(&token);
                            }
                            else {
                                poll.registry().reregister(&mut connection.client_socket, event.token(), connection.interest)?;
                            }
                        }
                    }
                }
            }
        }
    }
}



fn would_block(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::WouldBlock
}

fn main() -> Result<(), Box<dyn Error>> {

    let addr = "127.0.0.1:9999".parse()?;
    let mut server = EchoServer::new(addr)?;
    server.run()?;


    Ok(())
}
