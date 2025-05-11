defmodule NixRelayServer.ClientWebSocketHandler do
  def init(_params) do
    {:ok, %{}}
  end

  def handle_in({"job " <> rest, [opcode: :text]}, state) do
    [derivation, data] = String.split(rest, " ", parts: 2)
    IO.puts("received #{derivation}\n#{data}")
    NixRelayServer.BuildQueue.add_job(self(), derivation, data)
    {:ok, state}
  end

  def handle_info({:complete, derivation, success}, state) do
    {:push, {:text, derivation <> " " <> to_string(success)}, state}
  end

  # Invoked when the connection is closed
  def terminate(_reason, _state) do
    IO.puts("Client WebSocket connection closed")
    :ok
  end
end
