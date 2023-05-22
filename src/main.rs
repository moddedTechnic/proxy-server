use std::fs::File;
use std::io::{self, Read, Write};
use std::net::{ToSocketAddrs, TcpListener, TcpStream, SocketAddr, IpAddr};
use std::path::Path;
use std::thread;


#[derive(Debug)]
enum ClientError {
    Utf8Error(std::string::FromUtf8Error),
    IOError(std::io::Error),
    ParseIntError(std::num::ParseIntError),
    NoHostFound,
    SelfRequested,
}


fn read_stream(stream: &mut TcpStream) -> Result<String, ClientError> {
    let mut buffer = [0; 1024];
    let mut result = String::new();

    loop {
        let n = match stream.read(&mut buffer) {
            Ok(n) => n,
            Err(err) => return Err(ClientError::IOError(err)),
        };
        
        if n == 0 { break; }
        
        match String::from_utf8(buffer[..n].to_owned()) {
            Ok(s) => result += &s,
            Err(err) => return Err(ClientError::Utf8Error(err)),
        };

        if n < 1024 { break; }
    };

    return Ok(result);
}

fn get_host(request: &String) -> Result<(String, u16), ClientError> {
    let host = match request.lines().find(|line| line.starts_with("Host")) {
        Some(host) => match host.trim().split_whitespace().last() {
            Some(host) => host,
            None => return Err(ClientError::NoHostFound),
        },
        None => return Err(ClientError::NoHostFound),
    };

    let address_parts: Vec<&str> = host.split(":").collect();
    if address_parts.len() == 1 {
        Ok((address_parts[0].to_owned(), 80))
    } else if address_parts.len() == 2 {
        Ok((
            address_parts[0].to_owned(),
            match address_parts[1].parse::<u16>() {
                Ok(port) => port,
                Err(e) => return Err(ClientError::ParseIntError(e))
            }
        ))
    } else {
        Err(ClientError::NoHostFound)
    }
}

fn dns_lookup(address: (String, u16)) -> Result<Option<SocketAddr>, ClientError> {
    let mut dns_results = match address.to_socket_addrs() {
        Ok(results) => results,
        Err(_) => return Err(ClientError::NoHostFound),
    };
    Ok(dns_results.next())
}

fn perform_redirect(mut stream: TcpStream, redirect_address: SocketAddr, request: String) -> Result<(), ClientError> {
    println!("Forwarding request to {}", redirect_address);
    let mut redirect_stream = match TcpStream::connect(redirect_address) {
        Ok(result) => result,
        Err(err) => return Err(ClientError::IOError(err)),
    };
    match redirect_stream.write_all(request.as_bytes()) {
        Ok(_) => (),
        Err(err) => return Err(ClientError::IOError(err)),
    };
    let response = read_stream(&mut redirect_stream);
    match response {
        Ok(response) => match stream.write_all(response.as_bytes()) {
            Ok(_) => Ok(()),
            Err(err) => {
                Err(ClientError::IOError(err))
            },
        },
        Err(err) => {
            Err(err)
        },
    }
}

fn send_response(stream: &mut TcpStream, response: String) -> std::io::Result<()> {
    stream.write_all(response.as_bytes())
}

fn send_response_file(stream: &mut TcpStream, response_name: &str) -> std::io::Result<()> {
    let file_path = Path::new("responses").join(response_name.to_owned() + ".http");
    let mut file = File::open(file_path)?;
    
    let mut file_contents = String::new();
    file.read_to_string(&mut file_contents)?;

    send_response(stream, file_contents)
}

fn handle_client(mut stream: TcpStream, server_address: SocketAddr) -> Result<(), ClientError> {
    let request_text = read_stream(&mut stream)?;
    let address = get_host(&request_text)?;
    let redirect_address = dns_lookup(address)?;

    match redirect_address {
        Some(redirect_address) =>
            if redirect_address == server_address {
                return match send_response_file(&mut stream, "error508") {
                    Ok(_) => Err(ClientError::SelfRequested),
                    Err(e) => Err(ClientError::IOError(e))
                }
            } else {
                perform_redirect(stream, redirect_address, request_text)
            },
        None => return Err(ClientError::NoHostFound),
    }
}

fn main() -> io::Result<()> {
    let ip_address = match "127.0.0.1".parse::<IpAddr>() {
        Ok(addr) => addr,
        Err(e) => panic!("An error occurred: {}", e),
    };
    let server_address = SocketAddr::new(ip_address, 8080);
    let listener = TcpListener::bind("127.0.0.1:8080")?;

    for stream in listener.incoming() {
        let stream = stream?;
        thread::spawn(move || {
            match handle_client(stream, server_address) {
                Ok(_) => (),
                Err(e) => eprintln!("An error occurred: {:?}", e)
            };
        });
    }

    Ok(())
}
