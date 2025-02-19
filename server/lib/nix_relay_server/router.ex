defmodule NixRelayServer.Router do
  use Plug.Router

  plug(:match)
  plug(:dispatch)

  get "/worker" do
    conn
    |> WebSockAdapter.upgrade(NixRelayServer.WorkerWebSocketHandler, [], [])
    |> halt()
  end

  get "/client" do
    conn
    |> WebSockAdapter.upgrade(NixRelayServer.ClientWebSocketHandler, [], [])
    |> halt()
  end

  post "/upload/:hash" do
    hash = conn.params["hash"]

    # :ok = File.write!(nix_cache/info/#{hash}.nar.xz)
    # :ok = File.write!(nix_cache/info/#{hash}.narinfo)
    IO.puts("uploaded #{hash}")
    send_resp(conn, 200, "Uploaded #{hash}")
  end

  get "/:hash.narinfo" do
    hash = conn.params["hash"]
    IO.inspect(conn, label: "Test")
    file_path = "/tmp/nix_cache/info/#{hash}.narinfo"
    IO.puts("narhash #{hash}")

    case File.read(file_path) do
      {:ok, content} ->
        send_resp(conn, 200, content)

      {:error, _} ->
        NixRelayServer.BuildQueue.add_job(hash)
        NixRelayServer.BuildQueue.register_waiting_client(hash, self())

        receive do
          {:build_complete, ^hash, true} ->
            send_resp(conn, 200, Cache.get!(hash))

          {:build_complete, ^hash, false} ->
            send_resp(conn, 500, "Build failed")
        after
          300_000 -> send_resp(conn, 504, "Timeout")
        end
    end
  end

  get "/:hash.nar.xz" do
    hash = conn.params["hash"]
    file_path = "/tmp/nix_cache/nar/#{hash}.nar.xz"
    IO.puts("nar #{hash}")

    case File.read(file_path) do
      {:ok, content} ->
        conn
        |> put_resp_header("content-type", "application/x-xz")
        |> send_resp(200, content)

      {:error, _} ->
        NixRelayServer.BuildQueue.add_job(hash)
        NixRelayServer.BuildQueue.register_waiting_client(hash, self())

        receive do
          {:build_complete, ^hash, true} ->
            send_resp(conn, 200, Cache.get!(hash))

          {:build_complete, ^hash, false} ->
            send_resp(conn, 500, "Build failed")
        after
          300_000 -> send_resp(conn, 504, "Timeout")
        end
    end
  end

  get "/nix-cache-info" do
    IO.puts("cache info")
    send_resp(conn, 200, "Storedir: /nix/store")
  end

  get ":a" do
    a = conn.params["a"]
    IO.puts("unknown #{a}")
    send_resp(conn, 404, "Not found")
  end

  match _ do
    IO.puts("unknown request recieved")
    send_resp(conn, 404, "Not found")
  end
end
