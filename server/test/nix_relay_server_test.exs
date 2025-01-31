defmodule NixRelayServerTest do
  use ExUnit.Case
  doctest NixRelayServer

  test "greets the world" do
    assert NixRelayServer.hello() == :world
  end
end
