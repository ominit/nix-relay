defmodule NixRelayServer.ApplicationTest do
  use ExUnit.Case, async: false

  test "application starts successfully" do
    # Stop the application if it's running
    Application.stop(:nix_relay_server)

    # Start the application
    {:ok, _} = Application.ensure_all_started(:nix_relay_server)

    # Verify that our supervision tree is running
    assert Process.whereis(NixRelayServer.Supervisor) != nil
    assert Process.whereis(NixRelayServer.BuildQueue) != nil

    # Verify that Bandit is running and listening on port 4000
    # This is a simple way to check if the HTTP server is running
    {:ok, socket} = :gen_tcp.connect(~c"localhost", 4000, [:binary, active: false])
    :gen_tcp.close(socket)

    Application.stop(:nix_relay_server)
  end
end
