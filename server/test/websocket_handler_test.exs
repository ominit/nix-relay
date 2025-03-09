defmodule NixRelayServer.WebSocketHandlerTest do
  use ExUnit.Case, async: false

  setup do
    # Start BuildQueue for testing
    start_supervised!(NixRelayServer.BuildQueue)
    :ok
  end

  describe "ClientWebSocketHandler" do
    test "handles job submission" do
      # Create a test state
      state = %{}

      # Simulate a job message
      derivation = "test-derivation"
      data = "test-data"

      # Call the handler function directly
      result =
        NixRelayServer.ClientWebSocketHandler.handle_in(
          {"job #{derivation} #{data}", [opcode: :text]},
          state
        )

      # Verify the result
      assert result == {:ok, state}
    end

    test "handles completion messages" do
      # Create a test state
      state = %{}

      # Simulate a completion message
      derivation = "test-derivation"
      success = "true"

      # Call the handle_info function directly
      result =
        NixRelayServer.ClientWebSocketHandler.handle_info(
          {:complete, derivation, success},
          state
        )

      # Verify the result
      assert result == {:push, {:text, success}, state}
    end

    test "handles connection termination" do
      result = NixRelayServer.ClientWebSocketHandler.terminate(:normal, %{})
      assert result == :ok
    end
  end

  describe "WorkerWebSocketHandler" do
    test "handles register message" do
      # Create a test state
      state = %{}

      # Call the handler function directly
      result =
        NixRelayServer.WorkerWebSocketHandler.handle_in(
          {"register", [opcode: :text]},
          state
        )

      # Verify the result
      assert result == {:ok, state}
    end

    test "handles completion message" do
      # Create a test state
      state = "existing-derivation"

      # Call the handler function directly
      result =
        NixRelayServer.WorkerWebSocketHandler.handle_in(
          {"complete true test-derivation", [opcode: :text]},
          state
        )

      # Verify the result
      assert result == {:ok, state}
    end

    test "handles build request" do
      # Create a test state
      state = %{}
      derivation = "test-derivation"
      data = "test-data"
      client_pid = self()

      # Call the handle_info function directly
      result =
        NixRelayServer.WorkerWebSocketHandler.handle_info(
          {:request_build, {client_pid, derivation, data}},
          state
        )

      # Verify the result
      assert result == {:push, {:text, "request-build #{derivation} #{data}"}, derivation}
    end

    test "handles connection termination" do
      # Mock state with a derivation
      state = "test-derivation"

      # When a worker disconnects, it should be removed from the build queue
      result = NixRelayServer.WorkerWebSocketHandler.terminate(:normal, state)
      assert result == :ok

      # We can't easily test the internal state of BuildQueue here,
      # but the terminate function should complete successfully
    end
  end
end
