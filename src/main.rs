use std::{
    fs,
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    str,
};
use walkdir::WalkDir;
use percent_encoding::{percent_decode, utf8_percent_encode, NON_ALPHANUMERIC};


fn main() -> io::Result<()> {
    let current_dir = std::env::current_dir()?;
    println!("Current working directory: {:?}", current_dir);

    let listener = TcpListener::bind("127.0.0.1:8080")?;
    println!("Listening on http://127.0.0.1:8080");

    for stream in listener.incoming() {
        let stream = stream?;
        handle_connection(stream)?;
    }

    Ok(())
}

fn handle_connection(mut stream: TcpStream) -> io::Result<()> {
    let mut buffer = [0; 1024];
    stream.read(&mut buffer)?;

    let request = String::from_utf8_lossy(&buffer);
    let request_line = request.lines().next().unwrap_or("");
    let (method, path) = parse_request_line(request_line);

    if path == "/favicon.ico" {
        send_response(&mut stream, "404 Not Found", "text/html", "Not Found")?;
        return Ok(());
    }

    if method != "GET" {
        send_response(&mut stream, "405 Method Not Allowed", "text/html", "Method Not Allowed")?;
        return Ok(());
    }

    let decoded_path = decode_url_encoded(path);
    let resource_path = if decoded_path == "/" { "" } else { &decoded_path[1..] }; 
    let resource_path = Path::new(resource_path);
    let absolute_path = std::env::current_dir()?.join(resource_path);

    println!("Requested path: {:?}", resource_path);
    println!("Absolute path: {:?}", absolute_path);

    if !is_path_within_current_directory(&absolute_path)? {
        send_response(&mut stream, "403 Forbidden", "text/html", "Forbidden")?;
        return Ok(());
    }

    if absolute_path.is_dir() {
        send_directory_listing(&mut stream, &absolute_path)?;
    } else if absolute_path.is_file() {
        send_file_content(&mut stream, &absolute_path)?;
    } else {
        send_response(&mut stream, "404 Not Found", "text/html", "Not Found")?;
    }

    Ok(())
}


fn parse_request_line(request_line: &str) -> (&str, &str) {
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");
    (method, path)
}

fn is_path_within_current_directory(path: &Path) -> io::Result<bool> {
    let current_dir = std::env::current_dir()?;
    let abs_path = path.canonicalize()?;
    Ok(abs_path.starts_with(current_dir))
}

fn send_directory_listing(stream: &mut TcpStream, path: &Path) -> io::Result<()> {
    let mut response = String::new();
    response.push_str("<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head><body>");
    response.push_str(&format!("<h1>Directory listing for {}</h1>", decode_url_encoded(&path.display().to_string())));

    let entries = WalkDir::new(path).max_depth(1).min_depth(1);
    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy();

        // Here we strip the prefix relative to the requested directory, not the current directory
        let file_path = entry.path().strip_prefix(path)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?
            .display()
            .to_string();

        // Encode the file path to handle special characters (CJK characters, spaces, etc.)
        let encoded_file_path = encode_path(&file_path);

        // Add trailing slash for directories in the listing
        if entry.path().is_dir() {
            response.push_str(&format!("<a href=\"{}/\">{}/</a><br>", encoded_file_path, file_name));
        } else {
            response.push_str(&format!("<a href=\"{}\">{}</a><br>", encoded_file_path, file_name));
        }
    }

    response.push_str("</body></html>");
    send_response(stream, "200 OK", "text/html", &response)
}

fn send_file_content(stream: &mut TcpStream, path: &Path) -> io::Result<()> {
    let content = match fs::read(path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading file {}: {:?}", path.display(), e);
            send_response(stream, "500 Internal Server Error", "text/html", "Internal Server Error")?;
            return Ok(());
        }
    };

    let content_type = infer::get(&content).map_or("application/octet-stream", |mime| mime.mime_type());
    let content_length = content.len();
    
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
        content_type,
        content_length
    );
    
    stream.write_all(response.as_bytes())?;
    stream.write_all(&content)?;
    stream.flush()?;
    
    Ok(())
}


fn send_response(stream: &mut TcpStream, status: &str, content_type: &str, body: &str) -> io::Result<()> {
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n{}",
        status,
        content_type,
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    Ok(())
}

fn encode_path(path: &str) -> String {
    utf8_percent_encode(path, NON_ALPHANUMERIC).to_string()
}

fn decode_url_encoded(path: &str) -> String {
    percent_decode(path.as_bytes()).decode_utf8_lossy().to_string()
}