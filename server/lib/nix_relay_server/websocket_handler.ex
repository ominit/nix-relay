defmodule NixRelayServer.WebSocketHandler do
  def init(_params) do
    {:ok, %{}}
  end

  def handle_in({msg, [opcode: :text]}, state) do
    IO.puts("Received message: #{inspect(msg)}")
    {:ok, state}
  end

  # Invoked when the connection is closed
  def terminate(_reason, _state) do
    IO.puts("WebSocket connection closed")
    :ok
  end
end
