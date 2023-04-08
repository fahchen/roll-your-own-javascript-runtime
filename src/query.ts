import { JetRuntime } from "jet:runtime";

export async function handle(request: JetRuntime.Request, context: JetRuntime.Context): JetRuntime.Response {
  return new JetRuntime.Response(
    200,
    `Hello ${request.to}, this is ${context.current_user.name}.`,
  );
}
