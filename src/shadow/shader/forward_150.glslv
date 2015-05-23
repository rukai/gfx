#version 150 core

in vec3 a_Pos;
in vec3 a_Normal;

out vec3 v_Normal;
out vec3 v_Position;

uniform mat4 u_Transform;
uniform mat4 u_ModelTransform;
uniform mat3 u_NormalTransform;

void main() {
	v_Normal = u_NormalTransform * a_Normal;
	v_Position = (u_ModelTransform * vec4(a_Pos, 1.0)).xyz;
    gl_Position = u_Transform * vec4(a_Pos, 1.0);
}
